#![windows_subsystem = "windows"]

mod annotate;
mod config;
mod history;
mod history_ui;
mod imaging;
mod record;
mod settings_ui;
mod toast;
mod ui;

use std::{
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::{
    core::PCWSTR,
    Win32::{
        Foundation::*,
        Graphics::{
            Dwm::{DwmFlush, DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS},
            Gdi::*,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::WM_MOUSELEAVE, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
            WindowsAndMessaging::*,
        },
    },
};

const MAIN: &str = "SnippingLiteMain";
const OVERLAY: &str = "SnippingLiteOverlay";
const TOOLBAR: &str = "SnippingLiteToolbar";
const RECORD: &str = "SnippingLiteRecord";
const NEW: usize = 100;
const DELAY_TIMER: usize = 1;
const RECORD_TIMER: usize = 2;
const HOTKEY: i32 = 1;
const TRAY_MESSAGE: u32 = WM_APP + 1;
const TRAY_RELOAD: u32 = WM_APP + 2;
const TRAY_SETTINGS: usize = 201;
const TRAY_HISTORY: usize = 203;
const TRAY_EXIT: usize = 202;
const TRAY_STOP: usize = 204;
const MODE_RECT: i32 = 0;
const MODE_FREE: i32 = 1;
const MODE_WINDOW: i32 = 2;
const MODE_SCREEN: i32 = 3;
/// Selects a rectangle like `MODE_RECT`, but records it as a video.
const MODE_RECORD: i32 = 4;

struct State {
    owner: HWND,
    overlay: HWND,
    screen: HBITMAP,
    screen_dc: HDC,
    previous: HGDIOBJ,
    dim_canvas: Option<ui::Canvas>,
    toolbar: HWND,
    toolbar_dark: Option<bool>,
    active: bool,
    hover_button: i32,
    pressed_button: i32,
    hover_window: Option<RECT>,
    scale: f32,
    theme: ui::Theme,
    toast: Option<Box<toast::Toast>>,
    origin: POINT,
    width: i32,
    height: i32,
    mode: i32,
    start: POINT,
    end: POINT,
    points: Vec<POINT>,
    dragging: bool,
    hidden: Vec<&'static str>,
    copy: bool,
    save: bool,
    ask: bool,
    delay: u32,
    pending: bool,
    history: bool,
    hotkey_ok: bool,
    recording: Option<record::Recording>,
    record_window: HWND,
}
static mut STATE: *mut State = null_mut();
fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn state() -> &'static mut State {
    &mut *STATE
}
unsafe fn message(hwnd: HWND, text: &str) {
    MessageBoxW(
        hwnd,
        w(text).as_ptr(),
        w("Snipping Lite").as_ptr(),
        MB_OK | MB_ICONINFORMATION,
    );
}
const SETTINGS_TITLE: &str = "Snipping Lite Settings";
const HISTORY_TITLE: &str = "Snipping Lite History";
/// The window processes started by the tray, identified by their window titles.
const WINDOWS: [&str; 2] = [SETTINGS_TITLE, HISTORY_TITLE];
unsafe fn find_window(title: &str) -> HWND {
    FindWindowW(null(), w(title).as_ptr())
}
/// Opens a window process (`--settings` or `--history`), or brings the running one forward.
/// `latest` asks a new history window to open the newest screenshot.
unsafe fn launch_window(title: &str, argument: &str, latest: bool) {
    let window = find_window(title);
    if !window.is_null() {
        ShowWindow(window, SW_SHOW);
        SetForegroundWindow(window);
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe)
            .arg(argument)
            .args(latest.then_some("--latest"))
            .arg(if state().hotkey_ok {
                "--hotkey-ok"
            } else {
                "--hotkey-busy"
            })
            .spawn();
    }
}
unsafe fn launch_settings() {
    launch_window(SETTINGS_TITLE, "--settings", false);
}
unsafe fn launch_history(latest: bool) {
    launch_window(HISTORY_TITLE, "--history", latest);
}
unsafe fn begin_capture() {
    // The hotkey and the tray icon end a running recording.
    if state().recording.is_some() {
        stop_recording();
        return;
    }
    if state().active || state().pending {
        return;
    }
    let owner = state().owner;
    // Our own windows must not appear in the capture; they come back afterwards.
    state().hidden.clear();
    for title in WINDOWS {
        let window = find_window(title);
        if !window.is_null() && IsWindowVisible(window) != 0 {
            ShowWindow(window, SW_HIDE);
            state().hidden.push(title);
        }
    }
    // Only wait for the compositor when one of our windows has to leave the screen first.
    let flush = !state().hidden.is_empty() || toast::visible();
    toast::close();
    if state().delay > 0 && SetTimer(owner, DELAY_TIMER, state().delay * 1000, None) != 0 {
        state().pending = true;
        return;
    }
    capture(flush);
}
unsafe fn capture(flush: bool) {
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
    if !dimmed {
        cleanup();
        restore_windows();
        message(owner, "Could not capture the desktop.");
        return;
    }
    state().points.clear();
    state().hover_window = None;
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
    if state().mode == MODE_WINDOW {
        hover_window(POINT {
            x: cursor.x - x,
            y: cursor.y - y,
        });
    }
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
unsafe fn dim(width: i32, height: i32) -> bool {
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
    s.active = false;
    s.dragging = false;
    s.points.clear();
    s.points.shrink_to_fit();
    s.hover_window = None;
    s.hover_button = -1;
    s.pressed_button = -1;
}
fn point(lp: LPARAM) -> POINT {
    POINT {
        x: (lp as u32 as u16 as i16) as i32,
        y: ((lp as u32 >> 16) as u16 as i16) as i32,
    }
}
fn bounds(a: POINT, b: POINT, width: i32, height: i32) -> RECT {
    RECT {
        left: a.x.min(b.x).clamp(0, width),
        top: a.y.min(b.y).clamp(0, height),
        right: a.x.max(b.x).clamp(0, width),
        bottom: a.y.max(b.y).clamp(0, height),
    }
}
fn inflate(r: RECT, by: i32) -> RECT {
    RECT {
        left: r.left - by,
        top: r.top - by,
        right: r.right + by,
        bottom: r.bottom + by,
    }
}
/// Toolbar buttons in DIPs: four snip modes, then Close after a divider.
const BUTTON: f32 = 40.0;
const BAR_PADDING: f32 = 4.0;
const BAR_GAP: f32 = 4.0;
const DIVIDER: f32 = 9.0;
const BAR_HEIGHT: f32 = BUTTON + 2.0 * BAR_PADDING;
const BAR_WIDTH: f32 = 2.0 * BAR_PADDING + 6.0 * BUTTON + 4.0 * BAR_GAP + DIVIDER;
const GLYPHS: [&str; 6] = [
    "\u{EF20}", "\u{EA49}", "\u{E7C4}", "\u{E7F4}", "\u{E714}", "\u{E711}",
];
const CLOSE_BUTTON: i32 = 5;
fn toolbar_button(index: i32, scale: f32) -> [f32; 4] {
    let mut left = BAR_PADDING + index as f32 * (BUTTON + BAR_GAP);
    if index == CLOSE_BUTTON {
        left += DIVIDER - BAR_GAP;
    }
    [
        left * scale,
        BAR_PADDING * scale,
        (left + BUTTON) * scale,
        (BAR_PADDING + BUTTON) * scale,
    ]
}
/// Creates the hidden overlay and toolbar once; they are reused for every capture.
unsafe fn create_windows() -> bool {
    let s = state();
    if s.overlay.is_null() {
        s.overlay = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w(OVERLAY).as_ptr(),
            w("Snipping Lite").as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
    }
    if !s.overlay.is_null() && s.toolbar.is_null() {
        s.toolbar = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w(TOOLBAR).as_ptr(),
            w("Snipping Lite").as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            s.overlay,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
    }
    !s.overlay.is_null() && !s.toolbar.is_null()
}
unsafe fn place_toolbar(cursor: POINT) {
    let scale = state().scale;
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(
        MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST),
        &mut info,
    );
    let width = (BAR_WIDTH * scale).round() as i32;
    let height = (BAR_HEIGHT * scale).round() as i32;
    let monitor = info.rcMonitor;
    // DWM attribute calls are slow, so only repeat them when the theme changes.
    if state().toolbar_dark != Some(state().theme.dark) {
        ui::acrylic(state().toolbar, state().theme.dark);
        state().toolbar_dark = Some(state().theme.dark);
    }
    SetWindowPos(
        state().toolbar,
        null_mut(),
        (monitor.left + monitor.right - width) / 2,
        monitor.top + (12.0 * scale).round() as i32,
        width,
        height,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
}
unsafe fn select_window(p: POINT) -> Option<RECT> {
    let absolute = POINT {
        x: p.x + state().origin.x,
        y: p.y + state().origin.y,
    };
    let mut window = GetWindow(state().overlay, GW_HWNDNEXT);
    let inside = |r: &RECT| {
        absolute.x >= r.left && absolute.x < r.right && absolute.y >= r.top && absolute.y < r.bottom
    };
    while !window.is_null() {
        let mut rect: RECT = std::mem::zeroed();
        let mut cloaked = 0u32;
        // Cheap local checks first: DWM queries are cross-process calls and
        // most windows in the z-order are hidden.
        if window != state().owner
            && window != state().toolbar
            && IsWindowVisible(window) != 0
            && IsIconic(window) == 0
            && GetWindowRect(window, &mut rect) != 0
            && inside(&rect)
            && (DwmGetWindowAttribute(
                window,
                DWMWA_CLOAKED as u32,
                (&mut cloaked as *mut u32).cast(),
                4,
            ) != 0
                || cloaked == 0)
            && DwmGetWindowAttribute(
                window,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut rect as *mut RECT).cast(),
                std::mem::size_of::<RECT>() as u32,
            ) == 0
            && inside(&rect)
        {
            return Some(RECT {
                left: (rect.left - state().origin.x).max(0),
                top: (rect.top - state().origin.y).max(0),
                right: (rect.right - state().origin.x).min(state().width),
                bottom: (rect.bottom - state().origin.y).min(state().height),
            });
        }
        window = GetWindow(window, GW_HWNDNEXT);
    }
    None
}
fn same(a: Option<RECT>, b: Option<RECT>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            (a.left, a.top, a.right, a.bottom) == (b.left, b.top, b.right, b.bottom)
        }
        (None, None) => true,
        _ => false,
    }
}
unsafe fn border() -> i32 {
    (state().scale * 2.0).round().max(1.0) as i32
}
unsafe fn invalidate(r: RECT) {
    let r = inflate(r, border() + 1);
    InvalidateRect(state().overlay, &r, 0);
}
unsafe fn hover_window(p: POINT) {
    let hover = select_window(p);
    if !same(hover, state().hover_window) {
        if let Some(old) = state().hover_window {
            invalidate(old);
        }
        if let Some(new) = hover {
            invalidate(new);
        }
        state().hover_window = hover;
    }
}
unsafe fn set_mode(mode: i32) {
    if let Some(old) = state().hover_window.take() {
        invalidate(old);
    }
    state().mode = mode;
    InvalidateRect(state().toolbar, null(), 0);
    if mode == MODE_SCREEN {
        finish(
            RECT {
                left: 0,
                top: 0,
                right: state().width,
                bottom: state().height,
            },
            false,
        );
    } else if mode == MODE_WINDOW {
        let mut cursor = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        hover_window(POINT {
            x: cursor.x - state().origin.x,
            y: cursor.y - state().origin.y,
        });
    }
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
unsafe fn finish(rect: RECT, freeform: bool) {
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
        message(state().owner, "Could not create the capture.");
        return;
    }
    let old = SelectObject(mem, bitmap);
    let copied = BitBlt(
        mem,
        0,
        0,
        width,
        height,
        state().screen_dc,
        rect.left,
        rect.top,
        SRCCOPY,
    ) != 0;
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
        message(state().owner, "Could not copy the selected pixels.");
        return;
    };
    let mut errors = Vec::new();
    let mut saved = None;
    if state().save {
        match save_image(&pixels, width, height, state().ask) {
            Ok(path) => saved = path,
            Err(_) => errors.push("Could not save the image."),
        }
    }
    let copied = state().copy && imaging::copy_to_clipboard(state().owner, &pixels, width, height);
    if state().copy && !copied {
        errors.push("Could not copy the image to the clipboard.");
    }
    if errors.is_empty() {
        toast::show(&pixels, width, height, copied, saved);
    } else {
        message(state().owner, &errors.join("\n"));
    }
    // The history is written last so it never delays the clipboard or the notification.
    if state().history {
        let _ = history::add(&pixels, width, height);
    }
}
unsafe fn cancel() {
    if state().active {
        ReleaseCapture();
        ShowWindow(state().toolbar, SW_HIDE);
        ShowWindow(state().overlay, SW_HIDE);
    }
    cleanup();
    restore_windows();
}
unsafe fn restore_windows() {
    for title in std::mem::take(&mut state().hidden) {
        let window = find_window(title);
        if !window.is_null() {
            ShowWindow(window, SW_SHOW);
        }
    }
}
/// The undimmed part of the overlay, if any.
unsafe fn highlight() -> Option<RECT> {
    let s = state();
    match s.mode {
        MODE_RECT | MODE_RECORD if s.dragging => Some(bounds(s.start, s.end, s.width, s.height)),
        MODE_WINDOW => s.hover_window,
        _ => None,
    }
}
unsafe fn paint_overlay(dc: HDC, area: RECT) {
    let s = state();
    let Some(dim) = s.dim_canvas.as_ref() else {
        return;
    };
    let bright = highlight().filter(|r| r.right > r.left && r.bottom > r.top);
    let saved = SaveDC(dc);
    if let Some(b) = bright {
        ExcludeClipRect(dc, b.left, b.top, b.right, b.bottom);
    }
    BitBlt(
        dc,
        area.left,
        area.top,
        area.right - area.left,
        area.bottom - area.top,
        dim.dc(),
        area.left,
        area.top,
        SRCCOPY,
    );
    RestoreDC(dc, saved);
    let t = border();
    if let Some(b) = bright {
        BitBlt(
            dc,
            b.left,
            b.top,
            b.right - b.left,
            b.bottom - b.top,
            s.screen_dc,
            b.left,
            b.top,
            SRCCOPY,
        );
        let white = GetStockObject(WHITE_BRUSH) as HBRUSH;
        for edge in [
            RECT {
                left: b.left - t,
                top: b.top - t,
                right: b.right + t,
                bottom: b.top,
            },
            RECT {
                left: b.left - t,
                top: b.bottom,
                right: b.right + t,
                bottom: b.bottom + t,
            },
            RECT {
                left: b.left - t,
                top: b.top,
                right: b.left,
                bottom: b.bottom,
            },
            RECT {
                left: b.right,
                top: b.top,
                right: b.right + t,
                bottom: b.bottom,
            },
        ] {
            FillRect(dc, &edge, white);
        }
    }
    if s.mode == MODE_FREE && s.dragging && s.points.len() > 1 {
        let pen = CreatePen(PS_SOLID, t, 0x00ffffff);
        let old = SelectObject(dc, pen);
        Polyline(dc, s.points.as_ptr(), s.points.len() as i32);
        SelectObject(dc, old);
        DeleteObject(pen);
    }
}
unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            let p = point(lp);
            if state().mode == MODE_WINDOW {
                if let Some(rect) = state().hover_window {
                    finish(rect, false);
                }
                return 0;
            }
            state().start = p;
            state().end = p;
            state().points.clear();
            state().points.push(p);
            state().dragging = true;
            SetCapture(hwnd);
            0
        }
        WM_MOUSEMOVE if state().dragging => {
            let p = point(lp);
            if state().mode == MODE_FREE {
                if let Some(&last) = state().points.last() {
                    invalidate(bounds(last, p, state().width, state().height));
                }
                state().points.push(p);
            } else {
                let old = highlight();
                state().end = p;
                if let Some(r) = old {
                    invalidate(r);
                }
                if let Some(r) = highlight() {
                    invalidate(r);
                }
            }
            0
        }
        WM_MOUSEMOVE if state().mode == MODE_WINDOW => {
            hover_window(point(lp));
            0
        }
        WM_LBUTTONUP if state().dragging => {
            state().end = point(lp);
            state().dragging = false;
            ReleaseCapture();
            let free = state().mode == MODE_FREE;
            let rect = if free && state().points.len() >= 3 {
                let mut r = RECT {
                    left: state().width,
                    top: state().height,
                    right: 0,
                    bottom: 0,
                };
                for p in &state().points {
                    r.left = r.left.min(p.x);
                    r.top = r.top.min(p.y);
                    r.right = r.right.max(p.x);
                    r.bottom = r.bottom.max(p.y);
                }
                bounds(
                    POINT {
                        x: r.left,
                        y: r.top,
                    },
                    POINT {
                        x: r.right,
                        y: r.bottom,
                    },
                    state().width,
                    state().height,
                )
            } else {
                bounds(state().start, state().end, state().width, state().height)
            };
            if state().mode == MODE_RECORD {
                start_recording(rect);
            } else if !free || state().points.len() >= 3 {
                finish(rect, free);
            } else {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_KEYDOWN if wp as u32 == VK_ESCAPE as u32 => {
            cancel();
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint_overlay(dc, ps.rcPaint);
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
/// Recording indicator in DIPs: a red dot, the elapsed time and a Stop label.
const RECORD_WIDTH: f32 = 168.0;
const RECORD_HEIGHT: f32 = 48.0;
/// Starts recording the selected rectangle (overlay coordinates).
unsafe fn start_recording(rect: RECT) {
    if rect.right - rect.left < 16 || rect.bottom - rect.top < 16 {
        return;
    }
    let origin = state().origin;
    let area = RECT {
        left: rect.left + origin.x,
        top: rect.top + origin.y,
        right: rect.right + origin.x,
        bottom: rect.bottom + origin.y,
    };
    let path = record::video_folder()
        .and_then(|folder| Ok(folder.join(format!("Snipping Lite {}.mp4", imaging::timestamp()?))));
    // The overlay closes first; our own windows stay hidden until the recording ends.
    let hidden = std::mem::take(&mut state().hidden);
    cancel();
    DwmFlush();
    let started = match path {
        Ok(path) => record::start(area, path),
        Err(_) => Err("Could not create the recordings folder.".to_string()),
    };
    state().hidden = hidden;
    match started {
        Ok(recording) => {
            state().recording = Some(recording);
            let center = POINT {
                x: (area.left + area.right) / 2,
                y: (area.top + area.bottom) / 2,
            };
            show_indicator(center);
        }
        Err(error) => {
            restore_windows();
            message(state().owner, &error);
        }
    }
}
unsafe fn show_indicator(center: POINT) {
    let theme = ui::theme();
    let scale = ui::scale_at(center);
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(
        MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST),
        &mut info,
    );
    let width = (RECORD_WIDTH * scale).round() as i32;
    let height = (RECORD_HEIGHT * scale).round() as i32;
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w(RECORD).as_ptr(),
        w("Snipping Lite").as_ptr(),
        WS_POPUP,
        (info.rcMonitor.left + info.rcMonitor.right - width) / 2,
        info.rcMonitor.top + (12.0 * scale).round() as i32,
        width,
        height,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    if hwnd.is_null() {
        return;
    }
    // The indicator must not appear in the video it controls.
    SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    ui::acrylic(hwnd, theme.dark);
    ui::disable_transitions(hwnd);
    state().theme = theme;
    state().scale = scale;
    state().record_window = hwnd;
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(hwnd, RECORD_TIMER, 500, None);
}
/// Ends the recording, restores our windows and announces the file.
unsafe fn stop_recording() {
    let Some(recording) = state().recording.take() else {
        return;
    };
    let window = std::mem::replace(&mut state().record_window, null_mut());
    if !window.is_null() {
        DestroyWindow(window);
    }
    let result = recording.stop();
    restore_windows();
    match result {
        Ok((path, frame)) => toast::show_recording(&frame.pixels, frame.width, frame.height, path),
        Err(error) => message(state().owner, &error),
    }
}
unsafe fn paint_record(hwnd: HWND, dc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let Some(mut canvas) = ui::Canvas::new(client.right, client.bottom) else {
        return;
    };
    let s = state();
    let (theme, scale) = (&s.theme, s.scale);
    let elapsed = s
        .recording
        .as_ref()
        .map_or(0, |r| r.started.elapsed().as_secs());
    let text = ui::Font::new(ui::TEXT_FONT, 14.0 * scale, FW_NORMAL as i32);
    canvas.fill_round_rect(
        [14.0 * scale, 18.0 * scale, 26.0 * scale, 30.0 * scale],
        6.0 * scale,
        0xffe81123,
    );
    canvas.text(
        &format!("{:02}:{:02}", elapsed / 60, elapsed % 60),
        &text,
        ui::rect(36.0 * scale, 0.0, 92.0 * scale, client.bottom as f32),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        theme.text,
    );
    canvas.fill_round_rect(
        [100.0 * scale, 8.0 * scale, 160.0 * scale, 40.0 * scale],
        4.0 * scale,
        theme.selected,
    );
    canvas.fill_round_rect(
        [110.0 * scale, 18.0 * scale, 122.0 * scale, 30.0 * scale],
        2.0 * scale,
        theme.text,
    );
    canvas.text(
        "Stop",
        &text,
        ui::rect(128.0 * scale, 0.0, 160.0 * scale, client.bottom as f32),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        theme.text,
    );
    canvas.present(dc);
}
unsafe extern "system" fn record_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint_record(hwnd, dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER if wp == RECORD_TIMER => {
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONUP => {
            stop_recording();
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe fn toolbar_hit(p: POINT) -> i32 {
    (0..GLYPHS.len() as i32)
        .find(|&i| {
            let [l, t, r, b] = toolbar_button(i, state().scale);
            (p.x as f32) >= l && (p.x as f32) < r && (p.y as f32) >= t && (p.y as f32) < b
        })
        .unwrap_or(-1)
}
unsafe fn paint_toolbar(hwnd: HWND, dc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let Some(mut canvas) = ui::Canvas::new(client.right, client.bottom) else {
        return;
    };
    let s = state();
    let theme = &s.theme;
    let scale = s.scale;
    let icon = ui::Font::new(ui::ICON_FONT, 16.0 * scale, FW_NORMAL as i32);
    for (i, glyph) in GLYPHS.iter().enumerate() {
        let i = i as i32;
        let r = toolbar_button(i, scale);
        let fill = if i == s.pressed_button {
            theme.pressed
        } else if i == s.hover_button {
            theme.hover
        } else if i == s.mode {
            theme.selected
        } else {
            0
        };
        if fill != 0 {
            canvas.fill_round_rect(r, 4.0 * scale, fill);
        }
        if i == s.mode {
            // Selection indicator below the active mode, as in WinUI segmented controls.
            let cx = (r[0] + r[2]) / 2.0;
            canvas.fill_round_rect(
                [cx - 8.0 * scale, r[3] - 3.0 * scale, cx + 8.0 * scale, r[3]],
                1.5 * scale,
                theme.accent,
            );
        }
        canvas.text(
            glyph,
            &icon,
            ui::rect(r[0], r[1], r[2], r[3]),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            theme.text,
        );
    }
    let divider =
        toolbar_button(MODE_RECORD, scale)[2] + (DIVIDER / 2.0 + BAR_GAP / 2.0 - 0.5) * scale;
    canvas.fill_round_rect(
        [
            divider,
            (BAR_PADDING + 8.0) * scale,
            divider + scale.max(1.0),
            (BAR_PADDING + BUTTON - 8.0) * scale,
        ],
        0.0,
        theme.divider,
    );
    canvas.present(dc);
}
unsafe extern "system" fn toolbar_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint_toolbar(hwnd, dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_MOUSEMOVE => {
            let hit = toolbar_hit(point(lp));
            if hit != state().hover_button {
                if state().hover_button < 0 {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut track);
                }
                state().hover_button = hit;
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_MOUSELEAVE => {
            state().hover_button = -1;
            state().pressed_button = -1;
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONDOWN => {
            state().pressed_button = toolbar_hit(point(lp));
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONUP => {
            let hit = toolbar_hit(point(lp));
            let pressed = std::mem::replace(&mut state().pressed_button, -1);
            if hit >= 0 && hit == pressed {
                if hit == CLOSE_BUTTON {
                    cancel();
                } else {
                    set_mode(hit);
                }
            } else {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            state().owner = hwnd;
            ui::disable_transitions(hwnd);
            tray(hwnd, NIM_ADD);
            state().hotkey_ok = RegisterHotKey(
                hwnd,
                HOTKEY,
                MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                b'S' as u32,
            ) != 0;
            0
        }
        TRAY_RELOAD => {
            let settings = config::load_settings();
            state().copy = settings.copy;
            state().save = settings.save;
            state().ask = settings.ask;
            state().delay = settings.delay;
            state().history = settings.history;
            0
        }
        WM_COMMAND => {
            match wp & 0xffff {
                NEW => begin_capture(),
                TRAY_SETTINGS => launch_settings(),
                TRAY_HISTORY => launch_history(false),
                TRAY_STOP => stop_recording(),
                TRAY_EXIT => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        TRAY_MESSAGE => {
            if lp as u32 == WM_LBUTTONUP {
                begin_capture();
            }
            if lp as u32 == WM_RBUTTONUP {
                let menu = CreatePopupMenu();
                if state().recording.is_some() {
                    AppendMenuW(menu, MF_STRING, TRAY_STOP, w("Stop recording").as_ptr());
                }
                AppendMenuW(menu, MF_STRING, NEW, w("New snip").as_ptr());
                AppendMenuW(menu, MF_STRING, TRAY_HISTORY, w("History").as_ptr());
                AppendMenuW(menu, MF_STRING, TRAY_SETTINGS, w("Settings").as_ptr());
                AppendMenuW(menu, MF_SEPARATOR, 0, null());
                AppendMenuW(menu, MF_STRING, TRAY_EXIT, w("Exit").as_ptr());
                let mut cursor: POINT = std::mem::zeroed();
                GetCursorPos(&mut cursor);
                SetForegroundWindow(hwnd);
                TrackPopupMenu(menu, TPM_RIGHTBUTTON, cursor.x, cursor.y, 0, hwnd, null());
                DestroyMenu(menu);
            }
            0
        }
        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_TIMER if wp == DELAY_TIMER => {
            KillTimer(hwnd, DELAY_TIMER);
            state().pending = false;
            capture(false);
            0
        }
        WM_HOTKEY if wp == HOTKEY as usize => {
            begin_capture();
            0
        }
        WM_DESTROY => {
            // Finish the video file so a recording is not lost when the app exits.
            if let Some(recording) = state().recording.take() {
                let _ = recording.stop();
            }
            toast::close();
            tray(hwnd, NIM_DELETE);
            UnregisterHotKey(hwnd, HOTKEY);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe fn tray(hwnd: HWND, operation: u32) {
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = TRAY_MESSAGE;
    data.hIcon = ui::app_icon(32);
    let title = w("Snipping Lite");
    data.szTip[..title.len()].copy_from_slice(&title);
    Shell_NotifyIconW(operation, &data);
}
unsafe fn register(name: &str, proc: WNDPROC, brush: HBRUSH, cursor: PCWSTR) {
    let class_name = w(name);
    let class = WNDCLASSW {
        lpfnWndProc: proc,
        hInstance: GetModuleHandleW(null()),
        lpszClassName: class_name.as_ptr(),
        hCursor: LoadCursorW(null_mut(), cursor),
        hbrBackground: brush,
        ..std::mem::zeroed()
    };
    RegisterClassW(&class);
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--history") {
        history_ui::run(args.iter().any(|a| a == "--latest"));
        return;
    }
    if args.iter().any(|a| a == "--settings") {
        settings_ui::run(args.iter().any(|a| a == "--hotkey-ok"));
        return;
    }
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let settings = config::load_settings();
        let mut app = Box::new(State {
            owner: null_mut(),
            overlay: null_mut(),
            screen: null_mut(),
            screen_dc: null_mut(),
            previous: null_mut(),
            dim_canvas: None,
            toolbar: null_mut(),
            toolbar_dark: None,
            active: false,
            hover_button: -1,
            pressed_button: -1,
            hover_window: None,
            scale: 1.0,
            theme: ui::theme(),
            toast: None,
            origin: POINT { x: 0, y: 0 },
            width: 0,
            height: 0,
            mode: MODE_RECT,
            start: POINT { x: 0, y: 0 },
            end: POINT { x: 0, y: 0 },
            points: Vec::new(),
            dragging: false,
            hidden: Vec::new(),
            copy: settings.copy,
            save: settings.save,
            ask: settings.ask,
            delay: settings.delay,
            pending: false,
            history: settings.history,
            hotkey_ok: false,
            recording: None,
            record_window: null_mut(),
        });
        STATE = &mut *app;
        register(MAIN, Some(main_proc), null_mut(), IDC_ARROW);
        register(OVERLAY, Some(overlay_proc), null_mut(), IDC_CROSS);
        register(TOOLBAR, Some(toolbar_proc), null_mut(), IDC_ARROW);
        register(RECORD, Some(record_proc), null_mut(), IDC_ARROW);
        register(toast::CLASS, Some(toast::proc), null_mut(), IDC_HAND);
        // The tray window is never shown; it receives the tray, hotkey and timer messages.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w(MAIN).as_ptr(),
            w("Snipping Lite").as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        create_windows();
        launch_settings();
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        STATE = null_mut();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_bounds_are_sorted_and_clipped() {
        let r = bounds(POINT { x: 140, y: 40 }, POINT { x: -10, y: 10 }, 100, 100);
        assert_eq!((r.left, r.top, r.right, r.bottom), (0, 10, 100, 40));
    }
}
