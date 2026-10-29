//! Screenshot capture: freezing the screen, cropping the selection and delivering the image.

use super::*;

pub(crate) unsafe fn begin_capture() {
    // The hotkey and the tray icon end a running recording.
    if state().recording.is_some() {
        stop_recording();
        return;
    }
    // The shortcut also cancels a running countdown.
    if !state().count_window.is_null() {
        cancel_countdown();
        return;
    }
    if state().active || state().pending {
        return;
    }
    let owner = state().owner;
    // Our own windows must not appear in the capture; they come back afterwards.
    state().hidden.clear();
    for identity in WINDOWS {
        let window = find_window(identity);
        if !window.is_null() && IsWindowVisible(window) != 0 {
            ShowWindow(window, SW_HIDE);
            state().hidden.push(identity);
        }
    }
    // A kept toolbar must not appear in the capture either; it comes back with the overlay.
    let kept = IsWindowVisible(state().toolbar) != 0;
    if kept {
        ShowWindow(state().toolbar, SW_HIDE);
    }
    // Only wait for the compositor when one of our windows has to leave the screen first.
    let flush = kept || !state().hidden.is_empty() || toast::visible();
    toast::close();
    if state().delay > 0 && SetTimer(owner, DELAY_TIMER, state().delay * 1000, None) != 0 {
        state().pending = true;
        return;
    }
    capture(flush);
}
pub(crate) unsafe fn capture(flush: bool) {
    let owner = state().owner;
    if flush {
        DwmFlush();
    }
    let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    state().origin = POINT { x, y };
    state().width = width;
    state().height = height;
    let live = !state().freeze;
    let dimmed = if live {
        state().live = ui::Canvas::new(width, height);
        state().live.is_some()
    } else {
        freeze_screen(x, y, width, height)
    };
    if !dimmed {
        cleanup();
        restore_windows();
        message(owner, tr("Could not capture the desktop."));
        return;
    }
    state().points.clear();
    state().hover_window = None;
    state().hover_hwnd = null_mut();
    state().theme = ui::theme();
    let mut cursor = POINT { x: 0, y: 0 };
    GetCursorPos(&mut cursor);
    state().scale = ui::scale_at(cursor);
    if !create_windows() {
        cleanup();
        restore_windows();
        return;
    }
    let (overlay, toolbar) = (state().overlay, state().toolbar);
    state().active = true;
    SetWindowPos(overlay, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
    place_toolbar(cursor);
    state().toolbar_open = state().keep_toolbar;
    set_layered(overlay, live);
    if live {
        live_draw(RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        });
    }
    // Paint while cloaked so the first visible frame is complete and never
    // shows the previous capture.
    ui::cloak(overlay, true);
    ShowWindow(overlay, SW_SHOW);
    UpdateWindow(overlay);
    ShowWindow(toolbar, SW_SHOWNOACTIVATE);
    UpdateWindow(toolbar);
    ui::cloak(overlay, false);
    SetForegroundWindow(overlay);
    SetFocus(overlay);
    if hovers_windows() {
        hover_window(POINT {
            x: cursor.x - x,
            y: cursor.y - y,
        });
    }
}
/// Copies the desktop into the capture bitmap and prepares the dim layer.
unsafe fn freeze_screen(x: i32, y: i32, width: i32, height: i32) -> bool {
    let dc = GetDC(null_mut());
    let captured = !dc.is_null()
        && store_bitmap(dc, width, height)
        && BitBlt(
            state().screen_dc,
            0,
            0,
            width,
            height,
            dc,
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        ) != 0;
    let dimmed = captured && dim(width, height);
    if !dc.is_null() {
        ReleaseDC(null_mut(), dc);
    }
    dimmed
}
/// Creates the full-screen capture bitmap selected into its own DC.
unsafe fn store_bitmap(dc: HDC, width: i32, height: i32) -> bool {
    let mem = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, width, height);
    if mem.is_null() || bitmap.is_null() {
        if !mem.is_null() {
            DeleteDC(mem);
        }
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        return false;
    }
    let s = state();
    s.previous = SelectObject(mem, bitmap);
    s.screen = bitmap;
    s.screen_dc = mem;
    true
}
/// Fills the dim layer with a darkened copy of the screen.
pub(crate) unsafe fn dim(width: i32, height: i32) -> bool {
    let s = state();
    let Some(mut canvas) = ui::Canvas::new(width, height) else {
        return false;
    };
    if BitBlt(canvas.dc(), 0, 0, width, height, s.screen_dc, 0, 0, SRCCOPY) == 0 {
        return false;
    }
    GdiFlush();
    // Halving every channel matches a 50% black overlay and is much faster
    // than AlphaBlend on a full-screen bitmap.
    for p in canvas.pixels() {
        *p = (*p >> 1) & 0x007f7f7f;
    }
    s.dim_canvas = Some(canvas);
    true
}
unsafe fn cleanup() {
    let s = state();
    if !s.screen_dc.is_null() {
        SelectObject(s.screen_dc, s.previous);
        DeleteDC(s.screen_dc);
        s.screen_dc = null_mut();
    }
    if !s.screen.is_null() {
        DeleteObject(s.screen);
        s.screen = null_mut();
    }
    s.dim_canvas = None;
    s.live = None;
    s.active = false;
    s.dragging = false;
    s.points.clear();
    s.points.shrink_to_fit();
    s.hover_window = None;
    s.hover_hwnd = null_mut();
    s.hover_button = -1;
    s.pressed_button = -1;
}
unsafe fn read_pixels(dc: HDC, bitmap: HBITMAP, width: i32, height: i32) -> Option<Vec<u8>> {
    let len = (width as usize)
        .checked_mul(4)?
        .checked_mul(height as usize)?;
    let mut pixels = vec![0u8; len];
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let ok = GetDIBits(
        dc,
        bitmap,
        0,
        height as u32,
        pixels.as_mut_ptr().cast(),
        &mut info,
        DIB_RGB_COLORS,
    );
    (ok != 0).then_some(pixels)
}
unsafe fn apply_freeform_mask(
    dc: HDC,
    pixels: &mut [u8],
    points: &[POINT],
    width: i32,
    height: i32,
) -> bool {
    let screen = GetDC(null_mut());
    let mask = CreateCompatibleBitmap(screen, width, height);
    ReleaseDC(null_mut(), screen);
    if mask.is_null() {
        return false;
    }
    let old = SelectObject(dc, mask);
    PatBlt(dc, 0, 0, width, height, BLACKNESS);
    let region = CreatePolygonRgn(points.as_ptr(), points.len() as i32, ALTERNATE);
    FillRgn(dc, region, GetStockObject(WHITE_BRUSH) as HBRUSH);
    DeleteObject(region);
    SelectObject(dc, old);
    let covered = read_pixels(dc, mask, width, height);
    DeleteObject(mask);
    let Some(covered) = covered else {
        return false;
    };
    for (pixel, inside) in pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(covered.as_chunks::<4>().0)
    {
        *pixel = if inside[0] | inside[1] | inside[2] != 0 {
            [pixel[0], pixel[1], pixel[2], 255]
        } else {
            [255, 255, 255, 0]
        };
    }
    true
}
/// Saves the capture to Pictures\\Screenshots; `None` means the user cancelled Save As.
unsafe fn save_image(
    pixels: &[u8],
    width: i32,
    height: i32,
    ask: bool,
) -> std::io::Result<Option<PathBuf>> {
    let folder = imaging::screenshot_folder()?;
    let name = format!("Snipping Lite {}.png", imaging::timestamp()?);
    let Some(path) = imaging::choose_path(null_mut(), &folder, &name, ask) else {
        return Ok(None);
    };
    imaging::encode(&path, pixels, width, height)?;
    Ok(Some(path))
}
pub(crate) unsafe fn finish(rect: RECT, freeform: bool) {
    if state().mode == MODE_WINDOW {
        let image =
            window_capture::snapshot(window_capture::Target::new(state().hover_hwnd, false));
        cancel();
        match image {
            Ok(image) => {
                let pixels = with_outline(image.pixels, image.width, image.height);
                deliver_capture(pixels, image.width, image.height);
            }
            Err(_) => message(state().owner, tr("Could not capture the selected window.")),
        }
        return;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 1 || height < 1 {
        return;
    }
    let dc = GetDC(null_mut());
    let mem = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, width, height);
    ReleaseDC(null_mut(), dc);
    if mem.is_null() || bitmap.is_null() {
        if !mem.is_null() {
            DeleteDC(mem);
        }
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        cancel();
        message(state().owner, tr("Could not create the capture."));
        return;
    }
    let old = SelectObject(mem, bitmap);
    let live = state().screen_dc.is_null();
    let (source, left, top, operation) = if live {
        // Our own overlay and toolbar must be off the screen before it is copied.
        ShowWindow(state().toolbar, SW_HIDE);
        ShowWindow(state().overlay, SW_HIDE);
        DwmFlush();
        (
            GetDC(null_mut()),
            rect.left + state().origin.x,
            rect.top + state().origin.y,
            SRCCOPY | CAPTUREBLT,
        )
    } else {
        (state().screen_dc, rect.left, rect.top, SRCCOPY)
    };
    let copied = BitBlt(mem, 0, 0, width, height, source, left, top, operation) != 0;
    if live {
        ReleaseDC(null_mut(), source);
    }
    SelectObject(mem, old);
    let mut pixels = if copied {
        read_pixels(mem, bitmap, width, height)
    } else {
        None
    };
    DeleteObject(bitmap);
    if let Some(data) = pixels.as_mut() {
        if freeform && state().points.len() >= 3 {
            let points: Vec<POINT> = state()
                .points
                .iter()
                .map(|p| POINT {
                    x: p.x - rect.left,
                    y: p.y - rect.top,
                })
                .collect();
            if !apply_freeform_mask(mem, data, &points, width, height) {
                pixels = None;
            }
        } else {
            data.as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|p| p[3] = 255);
        }
    }
    DeleteDC(mem);
    // Close the overlay first so the Save As dialog is not covered by it.
    cancel();
    let Some(pixels) = pixels else {
        message(state().owner, tr("Could not copy the selected pixels."));
        return;
    };
    // A freeform shape has no straight edge to draw the border on.
    let pixels = if freeform && state().points.len() >= 3 {
        pixels
    } else {
        with_outline(pixels, width, height)
    };
    deliver_capture(pixels, width, height);
}

/// Draws the border chosen in the settings.
unsafe fn with_outline(mut pixels: Vec<u8>, width: i32, height: i32) -> Vec<u8> {
    if state().outline {
        annotate::outline(
            &mut pixels,
            width,
            height,
            annotate::OUTLINE_WIDTHS[state().outline_width],
            annotate::COLORS[state().outline_color],
        );
    }
    pixels
}

unsafe fn deliver_capture(pixels: Vec<u8>, width: i32, height: i32) {
    let mut errors = Vec::new();
    let mut saved = None;
    if state().save {
        match save_image(&pixels, width, height, state().ask) {
            Ok(path) => saved = path,
            Err(_) => errors.push(tr("Could not save the image.")),
        }
    }
    let copied = state().copy && imaging::copy_to_clipboard(state().owner, &pixels, width, height);
    if state().copy && !copied {
        errors.push(tr("Could not copy the image to the clipboard."));
    }
    let png = saved.clone().filter(|path| {
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    });
    if errors.is_empty() {
        if state().notify {
            toast::show(&pixels, width, height, copied, saved);
        }
    } else {
        message(state().owner, &errors.join("\n"));
    }
    // The history is written last so it never delays the clipboard or the notification.
    if state().history {
        let _ = history::add(&pixels, width, height, png.as_deref());
    }
}
pub(crate) unsafe fn cancel() {
    if state().active {
        ReleaseCapture();
        ShowWindow(state().toolbar, SW_HIDE);
        ShowWindow(state().overlay, SW_HIDE);
    }
    cleanup();
    restore_windows();
}
pub(crate) unsafe fn restore_windows() {
    for identity in std::mem::take(&mut state().hidden) {
        let window = find_window(identity);
        if !window.is_null() {
            ShowWindow(window, SW_SHOW);
        }
    }
    let s = state();
    if s.toolbar_open
        && !s.active
        && !s.pending
        && s.recording.is_none()
        && s.count_window.is_null()
    {
        ShowWindow(s.toolbar, SW_SHOWNOACTIVATE);
    }
}
