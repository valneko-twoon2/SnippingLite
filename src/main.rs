#![windows_subsystem = "windows"]

mod annotate;
mod config;
mod history;
mod history_ui;
mod i18n;
mod imaging;
mod record;
mod settings_ui;
mod toast;
mod ui;

use i18n::tr;
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
const FRAME: &str = "SnippingLiteFrame";
const COUNT: &str = "SnippingLiteCount";
const NEW: usize = 100;
const DELAY_TIMER: usize = 1;
const RECORD_TIMER: usize = 2;
const COUNT_TIMER: usize = 3;
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
    video: bool,
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
    record_frame: HWND,
    tray_icon: HICON,
    /// The live overlay content; set instead of the frozen screen when `freeze` is off.
    live: Option<ui::Canvas>,
    freeze: bool,
    countdown: bool,
    skip_title: bool,
    record_cursor: bool,
    record_fps: u32,
    count_window: HWND,
    count_left: i32,
    count_area: RECT,
    count_points: Vec<POINT>,
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
// Both document windows share a caption; properties identify their roles.
const SETTINGS_TITLE: &str = "Snipping Lite";
const HISTORY_TITLE: &str = SETTINGS_TITLE;
const SETTINGS_WINDOW: &str = "SnippingLite.Settings";
const HISTORY_WINDOW: &str = "SnippingLite.History";
const WINDOWS: [&str; 2] = [SETTINGS_WINDOW, HISTORY_WINDOW];
unsafe fn find_window(identity: &str) -> HWND {
    ui::find_app_window(identity)
}
/// Opens a window process (`--settings` or `--history`), or brings the running one forward.
/// `latest` asks a new history window to open the newest screenshot.
unsafe fn launch_window(identity: &str, argument: &str, latest: bool) {
    let window = find_window(identity);
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
    launch_window(SETTINGS_WINDOW, "--settings", false);
}
unsafe fn launch_history(latest: bool) {
    launch_window(HISTORY_WINDOW, "--history", latest);
}
unsafe fn begin_capture() {
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
    s.live = None;
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
/// Fluent command bar: capture type, area picker, editing, history and settings.
const BAR_HEIGHT: f32 = 48.0;
const BAR_WIDTH: f32 = 332.0;
const CAMERA_BUTTON: i32 = 0;
const VIDEO_BUTTON: i32 = 1;
const AREA_BUTTON: i32 = 2;
const EDIT_BUTTON: i32 = 3;
const HISTORY_BUTTON: i32 = 4;
const SETTINGS_BUTTON: i32 = 5;
const CLOSE_BUTTON: i32 = 6;
const GLYPHS: [&str; 7] = [
    "\u{E722}", "\u{E714}", "\u{EF20}", "\u{E70F}", "\u{E81C}", "\u{E713}", "\u{E711}",
];
fn toolbar_button(index: i32, scale: f32) -> [f32; 4] {
    let (left, width) = match index {
        CAMERA_BUTTON => (8.0, 36.0),
        VIDEO_BUTTON => (44.0, 36.0),
        AREA_BUTTON => (92.0, 48.0),
        EDIT_BUTTON => (148.0, 36.0),
        HISTORY_BUTTON => (196.0, 36.0),
        SETTINGS_BUTTON => (236.0, 36.0),
        _ => (288.0, 36.0),
    };
    [
        left * scale,
        8.0 * scale,
        (left + width) * scale,
        40.0 * scale,
    ]
}

unsafe fn choose_capture_area(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    for (mode, label) in [
        (MODE_RECT, tr("Rectangle")),
        (MODE_FREE, tr("Freeform")),
        (MODE_WINDOW, tr("Window")),
        (MODE_SCREEN, tr("Full screen")),
    ] {
        let checked = if state().mode == mode { MF_CHECKED } else { 0 };
        AppendMenuW(
            menu,
            MF_STRING | checked,
            mode as usize + 1,
            w(label).as_ptr(),
        );
    }
    let button = toolbar_button(AREA_BUTTON, state().scale);
    let mut anchor = POINT {
        x: button[0] as i32,
        y: button[3] as i32,
    };
    ClientToScreen(hwnd, &mut anchor);
    SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY,
        anchor.x,
        anchor.y,
        0,
        hwnd,
        null(),
    );
    DestroyMenu(menu);
    if command > 0 {
        set_mode(command - 1);
    }
    if state().active {
        SetForegroundWindow(state().overlay);
    }
}
/// Creates the hidden overlay and toolbar once; they are reused for every capture.
unsafe fn create_windows() -> bool {
    let s = state();
    if s.overlay.is_null() {
        s.overlay = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w(OVERLAY).as_ptr(),
            w(OVERLAY).as_ptr(),
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
            w(TOOLBAR).as_ptr(),
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
            // Recording a window can leave out its title bar.
            if state().video && state().skip_title {
                rect.top = rect.top.max(title_bottom(window)).min(rect.bottom - 1);
            }
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
/// A pixel that receives mouse input while staying invisible on the live screen.
const LIVE_BASE: u32 = 0x04000000;
unsafe fn set_layered(hwnd: HWND, layered: bool) {
    let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    let wanted = if layered {
        style | WS_EX_LAYERED as isize
    } else {
        style & !(WS_EX_LAYERED as isize)
    };
    if wanted != style {
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
    }
}
fn fill_rect(canvas: &mut ui::Canvas, r: RECT, value: u32) {
    let (width, height) = (canvas.width, canvas.height);
    let (left, right) = (r.left.clamp(0, width), r.right.clamp(0, width));
    let (top, bottom) = (r.top.clamp(0, height), r.bottom.clamp(0, height));
    if right <= left {
        return;
    }
    let pixels = canvas.pixels();
    for y in top..bottom {
        pixels[(y * width + left) as usize..(y * width + right) as usize].fill(value);
    }
}
/// Fills the four edges of a frame of thickness `t` around `r`.
fn fill_frame(canvas: &mut ui::Canvas, r: RECT, t: i32, value: u32) {
    for edge in [
        RECT {
            left: r.left - t,
            top: r.top - t,
            right: r.right + t,
            bottom: r.top,
        },
        RECT {
            left: r.left - t,
            top: r.bottom,
            right: r.right + t,
            bottom: r.bottom + t,
        },
        RECT {
            left: r.left - t,
            top: r.top,
            right: r.left,
            bottom: r.bottom,
        },
        RECT {
            left: r.right,
            top: r.top,
            right: r.right + t,
            bottom: r.bottom,
        },
    ] {
        fill_rect(canvas, edge, value);
    }
}
/// Redraws the live overlay inside `dirty`: an invisible input layer with the
/// selection (outline and a light tint) or the freeform trail on top.
unsafe fn live_draw(dirty: RECT) {
    let t = border();
    let bright = highlight().filter(|r| r.right > r.left && r.bottom > r.top);
    let s = state();
    let Some(canvas) = s.live.as_mut() else {
        return;
    };
    let dirty = RECT {
        left: dirty.left.clamp(0, canvas.width),
        top: dirty.top.clamp(0, canvas.height),
        right: dirty.right.clamp(0, canvas.width),
        bottom: dirty.bottom.clamp(0, canvas.height),
    };
    fill_rect(canvas, dirty, LIVE_BASE);
    if let Some(b) = bright {
        fill_rect(canvas, b, 0x0c0c0c0c);
        fill_frame(canvas, inflate(b, 2), t, 0x80000000);
        fill_frame(canvas, b, t, 0xffffffff);
    }
    if s.mode == MODE_FREE && s.dragging && s.points.len() > 1 {
        for pair in s.points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let steps = (b.x - a.x).abs().max((b.y - a.y).abs()).max(1);
            for i in 0..=steps {
                let x = a.x + (b.x - a.x) * i / steps;
                let y = a.y + (b.y - a.y) * i / steps;
                fill_rect(
                    canvas,
                    RECT {
                        left: x - t / 2,
                        top: y - t / 2,
                        right: x - t / 2 + t,
                        bottom: y - t / 2 + t,
                    },
                    0xffffffff,
                );
            }
        }
    }
    let mut origin = s.origin;
    let mut size = SIZE {
        cx: canvas.width,
        cy: canvas.height,
    };
    let mut source = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let info = UPDATELAYEREDWINDOWINFO {
        cbSize: std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
        hdcDst: null_mut(),
        pptDst: &mut origin,
        psize: &mut size,
        hdcSrc: canvas.dc(),
        pptSrc: &mut source,
        crKey: 0,
        pblend: &blend,
        dwFlags: ULW_ALPHA,
        prcDirty: &dirty,
    };
    UpdateLayeredWindowIndirect(s.overlay, &info);
}
unsafe fn border() -> i32 {
    (state().scale * 2.0).round().max(1.0) as i32
}
unsafe fn invalidate(r: RECT) {
    let r = inflate(r, border() + 3);
    if state().live.is_some() {
        live_draw(r);
    } else {
        InvalidateRect(state().overlay, &r, 0);
    }
}
/// Repaints what a change from `old` to `new` touched; the state must already be updated.
unsafe fn invalidate_change(old: Option<RECT>, new: Option<RECT>) {
    match (old, new) {
        (Some(a), Some(b)) => invalidate(RECT {
            left: a.left.min(b.left),
            top: a.top.min(b.top),
            right: a.right.max(b.right),
            bottom: a.bottom.max(b.bottom),
        }),
        (Some(r), None) | (None, Some(r)) => invalidate(r),
        (None, None) => {}
    }
}
unsafe fn refresh_overlay() {
    if state().live.is_some() {
        live_draw(RECT {
            left: 0,
            top: 0,
            right: state().width,
            bottom: state().height,
        });
    } else {
        InvalidateRect(state().overlay, null(), 0);
    }
}
/// Window selection is explicit for both screenshots and recordings.
unsafe fn hovers_windows() -> bool {
    state().mode == MODE_WINDOW
}
/// Where the title bar ends, in screen coordinates.
unsafe fn title_bottom(window: HWND) -> i32 {
    let mut rect: RECT = std::mem::zeroed();
    GetWindowRect(window, &mut rect);
    let dpi = GetDpiForWindow(window);
    rect.top
        + GetSystemMetricsForDpi(SM_CYCAPTION, dpi)
        + GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi)
        + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
}
unsafe fn hover_window(p: POINT) {
    let hover = select_window(p);
    if !same(hover, state().hover_window) {
        let old = std::mem::replace(&mut state().hover_window, hover);
        invalidate_change(old, hover);
    }
}
unsafe fn set_mode(mode: i32) {
    if let Some(old) = state().hover_window.take() {
        invalidate(old);
    }
    state().mode = mode;
    InvalidateRect(state().toolbar, null(), 0);
    if mode == MODE_SCREEN {
        complete_selection(
            RECT {
                left: 0,
                top: 0,
                right: state().width,
                bottom: state().height,
            },
            false,
        );
    } else if hovers_windows() {
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
    for identity in std::mem::take(&mut state().hidden) {
        let window = find_window(identity);
        if !window.is_null() {
            ShowWindow(window, SW_SHOW);
        }
    }
}
/// The undimmed part of the overlay, if any.
unsafe fn highlight() -> Option<RECT> {
    let s = state();
    match s.mode {
        MODE_RECT if s.dragging => Some(bounds(s.start, s.end, s.width, s.height)),
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
            if state().mode == MODE_SCREEN {
                complete_selection(
                    RECT {
                        left: 0,
                        top: 0,
                        right: state().width,
                        bottom: state().height,
                    },
                    false,
                );
                return 0;
            }
            if state().mode == MODE_WINDOW {
                if let Some(rect) = state().hover_window {
                    complete_selection(rect, false);
                }
                return 0;
            }
            if let Some(old) = state().hover_window.take() {
                invalidate(old);
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
                let last = state().points.last().copied();
                state().points.push(p);
                if let Some(last) = last {
                    invalidate(bounds(last, p, state().width, state().height));
                }
            } else {
                let old = highlight();
                state().end = p;
                invalidate_change(old, highlight());
            }
            0
        }
        WM_MOUSEMOVE if hovers_windows() => {
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
            if !free || state().points.len() >= 3 {
                complete_selection(rect, free);
            } else {
                refresh_overlay();
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
/// Sends a selected shape to the active screenshot or recording destination.
unsafe fn complete_selection(rect: RECT, freeform: bool) {
    if state().video {
        start_recording(rect, freeform);
    } else {
        finish(rect, freeform);
    }
}

/// Starts recording a selected shape (overlay coordinates).
unsafe fn start_recording(rect: RECT, freeform: bool) {
    if rect.right - rect.left < 16 || rect.bottom - rect.top < 16 {
        refresh_overlay();
        return;
    }
    let points = if freeform {
        state()
            .points
            .iter()
            .map(|p| POINT {
                x: p.x - rect.left,
                y: p.y - rect.top,
            })
            .collect()
    } else {
        Vec::new()
    };
    let origin = state().origin;
    let area = RECT {
        left: rect.left + origin.x,
        top: rect.top + origin.y,
        right: rect.right + origin.x,
        bottom: rect.bottom + origin.y,
    };
    // The overlay closes first; our own windows stay hidden until the recording ends.
    let hidden = std::mem::take(&mut state().hidden);
    cancel();
    state().hidden = hidden;
    if state().countdown {
        begin_countdown(area, points);
    } else {
        begin_recording(area, points);
    }
}
unsafe fn begin_recording(area: RECT, points: Vec<POINT>) {
    DwmFlush();
    let started = record::video_folder()
        .map_err(|_| tr("Could not create the recordings folder.").to_string())
        .and_then(|folder| {
            let name = imaging::timestamp().map_err(|e| e.to_string())?;
            record::start(
                area,
                folder.join(format!("Snipping Lite {name}.mp4")),
                points,
                record::Options {
                    fps: state().record_fps,
                    cursor: state().record_cursor,
                },
            )
        });
    match started {
        Ok(recording) => {
            state().recording = Some(recording);
            let center = POINT {
                x: (area.left + area.right) / 2,
                y: (area.top + area.bottom) / 2,
            };
            show_indicator(center);
            show_frame(area);
        }
        Err(error) => {
            restore_windows();
            message(state().owner, &error);
        }
    }
}
/// A 3, 2, 1 countdown in the middle of the area; a click cancels it.
const COUNT_SIZE: f32 = 120.0;
unsafe fn begin_countdown(area: RECT, points: Vec<POINT>) {
    let center = POINT {
        x: (area.left + area.right) / 2,
        y: (area.top + area.bottom) / 2,
    };
    let theme = ui::theme();
    let scale = ui::scale_at(center);
    let size = (COUNT_SIZE * scale).round() as i32;
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w(COUNT).as_ptr(),
        w(COUNT).as_ptr(),
        WS_POPUP,
        center.x - size / 2,
        center.y - size / 2,
        size,
        size,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    if hwnd.is_null() {
        begin_recording(area, points);
        return;
    }
    SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    ui::acrylic(hwnd, theme.dark);
    ui::disable_transitions(hwnd);
    let s = state();
    s.theme = theme;
    s.scale = scale;
    s.count_window = hwnd;
    s.count_left = 3;
    s.count_area = area;
    s.count_points = points;
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(hwnd, COUNT_TIMER, 1000, None);
}
unsafe fn cancel_countdown() {
    state().count_points.clear();
    let window = std::mem::replace(&mut state().count_window, null_mut());
    if !window.is_null() {
        DestroyWindow(window);
    }
    restore_windows();
}
unsafe extern "system" fn count_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut client);
            if let Some(mut canvas) = ui::Canvas::new(client.right, client.bottom) {
                let s = state();
                let font = ui::Font::new(ui::TEXT_FONT, 56.0 * s.scale, FW_NORMAL as i32);
                canvas.text(
                    &s.count_left.to_string(),
                    &font,
                    client,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    s.theme.text,
                );
                canvas.present(dc);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER if wp == COUNT_TIMER => {
            state().count_left -= 1;
            if state().count_left <= 0 {
                let area = state().count_area;
                let points = std::mem::take(&mut state().count_points);
                state().count_window = null_mut();
                DestroyWindow(hwnd);
                begin_recording(area, points);
            } else {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_LBUTTONUP => {
            cancel_countdown();
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
/// Outlines the recorded area just outside of it; the frame is not captured.
unsafe fn show_frame(area: RECT) {
    let scale = ui::scale_at(POINT {
        x: (area.left + area.right) / 2,
        y: (area.top + area.bottom) / 2,
    });
    let thickness = (2.0 * scale).round().max(2.0) as i32;
    let (width, height) = (
        area.right - area.left + 2 * thickness,
        area.bottom - area.top + 2 * thickness,
    );
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
        w(FRAME).as_ptr(),
        w(FRAME).as_ptr(),
        WS_POPUP,
        area.left - thickness,
        area.top - thickness,
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
    let outer = CreateRectRgn(0, 0, width, height);
    let inner = CreateRectRgn(thickness, thickness, width - thickness, height - thickness);
    CombineRgn(outer, outer, inner, RGN_DIFF);
    DeleteObject(inner);
    // The window takes ownership of the region.
    SetWindowRgn(hwnd, outer, 0);
    SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    ui::disable_transitions(hwnd);
    state().record_frame = hwnd;
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
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
        w(RECORD).as_ptr(),
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
    for window in [
        std::mem::replace(&mut state().record_window, null_mut()),
        std::mem::replace(&mut state().record_frame, null_mut()),
    ] {
        if !window.is_null() {
            DestroyWindow(window);
        }
    }
    let result = recording.stop();
    restore_windows();
    match result {
        Ok((path, frame)) => {
            toast::show_recording(&frame.pixels, frame.width, frame.height, path);
            if let Some(warning) = frame.warning {
                let text = tr("Recording ended early; the video up to that point was saved.");
                message(state().owner, &format!("{text}\n{warning}"));
            }
        }
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
    let elapsed = s.recording.as_ref().map_or(0, |r| r.elapsed().as_secs());
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
        tr("Stop"),
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
            // A recorder that ended on its own failed; stopping reports the error.
            if state().recording.as_ref().is_some_and(|r| r.failed()) {
                stop_recording();
            } else {
                InvalidateRect(hwnd, null(), 0);
            }
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
    canvas.surface(theme.dark, scale);
    // The two capture types share one segmented outline.
    canvas.fill_round_rect(
        [8.0 * scale, 8.0 * scale, 80.0 * scale, 40.0 * scale],
        4.0 * scale,
        theme.divider,
    );
    canvas.fill_round_rect(
        [9.0 * scale, 9.0 * scale, 79.0 * scale, 39.0 * scale],
        3.0 * scale,
        if theme.dark { 0xff202020 } else { 0xfff3f3f3 },
    );
    for (i, glyph) in GLYPHS.iter().enumerate() {
        let i = i as i32;
        let r = toolbar_button(i, scale);
        let selected = (i == CAMERA_BUTTON && !s.video) || (i == VIDEO_BUTTON && s.video);
        let fill = if i == s.pressed_button {
            theme.pressed
        } else if i == s.hover_button {
            theme.hover
        } else if selected {
            theme.selected
        } else {
            0
        };
        if fill != 0 {
            canvas.fill_round_rect(r, 4.0 * scale, fill);
        }
        if selected {
            let cx = (r[0] + r[2]) / 2.0;
            canvas.fill_round_rect(
                [cx - 7.0 * scale, r[3] - 3.0 * scale, cx + 7.0 * scale, r[3]],
                1.5 * scale,
                theme.accent,
            );
        }
        let glyph = if i == AREA_BUTTON {
            match s.mode {
                MODE_FREE => "\u{EA49}",
                MODE_WINDOW => "\u{E7C4}",
                MODE_SCREEN => "\u{E7F4}",
                _ => glyph,
            }
        } else {
            glyph
        };
        let right = if i == AREA_BUTTON {
            r[2] - 14.0 * scale
        } else {
            r[2]
        };
        canvas.text(
            glyph,
            &icon,
            ui::rect(r[0], r[1], right, r[3]),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            theme.text,
        );
        if i == AREA_BUTTON {
            let chevron = ui::Font::new(ui::ICON_FONT, 8.0 * scale, FW_NORMAL as i32);
            canvas.text(
                "\u{E70D}",
                &chevron,
                ui::rect(right, r[1], r[2], r[3]),
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                theme.text,
            );
        }
    }
    for x in [188.0, 280.0] {
        canvas.fill_round_rect(
            [x * scale, 12.0 * scale, (x + 1.0) * scale, 36.0 * scale],
            0.0,
            theme.divider,
        );
    }
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
                match hit {
                    CLOSE_BUTTON => cancel(),
                    SETTINGS_BUTTON => {
                        cancel();
                        launch_settings();
                    }
                    EDIT_BUTTON | HISTORY_BUTTON => {
                        cancel();
                        launch_history(hit == EDIT_BUTTON);
                    }
                    CAMERA_BUTTON | VIDEO_BUTTON => {
                        state().video = hit == VIDEO_BUTTON;
                        // Refresh window bounds because recordings may exclude the title bar.
                        if hovers_windows() {
                            let mode = state().mode;
                            set_mode(mode);
                        }
                    }
                    AREA_BUTTON => choose_capture_area(hwnd),
                    _ => {}
                }
                InvalidateRect(hwnd, null(), 0);
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
            state().countdown = settings.countdown;
            state().skip_title = settings.skip_title;
            state().record_cursor = settings.record_cursor;
            state().record_fps = settings.record_fps;
            state().freeze = settings.freeze;
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
                    AppendMenuW(menu, MF_STRING, TRAY_STOP, w(tr("Stop recording")).as_ptr());
                }
                AppendMenuW(menu, MF_STRING, NEW, w(tr("New snip")).as_ptr());
                AppendMenuW(menu, MF_STRING, TRAY_HISTORY, w(tr("History")).as_ptr());
                AppendMenuW(menu, MF_STRING, TRAY_SETTINGS, w(tr("Settings")).as_ptr());
                AppendMenuW(menu, MF_SEPARATOR, 0, null());
                AppendMenuW(menu, MF_STRING, TRAY_EXIT, w(tr("Exit")).as_ptr());
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
    if state().tray_icon.is_null() {
        state().tray_icon = ui::app_icon(32);
    }
    data.hIcon = state().tray_icon;
    let title = w("Snipping Lite");
    data.szTip[..title.len()].copy_from_slice(&title);
    Shell_NotifyIconW(operation, &data);
    if operation == NIM_DELETE {
        DestroyIcon(state().tray_icon);
        state().tray_icon = null_mut();
    }
}
unsafe fn register(name: &str, proc: WNDPROC, brush: HBRUSH, cursor: PCWSTR) {
    let class_name = w(name);
    let class = WNDCLASSW {
        lpfnWndProc: proc,
        hInstance: GetModuleHandleW(null()),
        hIcon: LoadIconW(GetModuleHandleW(null()), 1usize as PCWSTR),
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
            video: false,
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
            record_frame: null_mut(),
            tray_icon: null_mut(),
            live: None,
            freeze: settings.freeze,
            countdown: settings.countdown,
            skip_title: settings.skip_title,
            record_cursor: settings.record_cursor,
            record_fps: settings.record_fps,
            count_window: null_mut(),
            count_left: 0,
            count_points: Vec::new(),
            count_area: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
        });
        STATE = &mut *app;
        register(MAIN, Some(main_proc), null_mut(), IDC_ARROW);
        register(OVERLAY, Some(overlay_proc), null_mut(), IDC_CROSS);
        register(TOOLBAR, Some(toolbar_proc), null_mut(), IDC_ARROW);
        register(RECORD, Some(record_proc), null_mut(), IDC_ARROW);
        register(COUNT, Some(count_proc), null_mut(), IDC_ARROW);
        // The frame around a recording is a solid red window shaped like a ring.
        register(
            FRAME,
            Some(DefWindowProcW),
            CreateSolidBrush(0x002311e8),
            IDC_ARROW,
        );
        register(toast::CLASS, Some(toast::proc), null_mut(), IDC_HAND);
        // The tray window is never shown; it receives the tray, hotkey and timer messages.
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w(MAIN).as_ptr(),
            w(MAIN).as_ptr(),
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
