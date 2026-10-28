#![windows_subsystem = "windows"]

mod toast;
mod ui;

use std::{
    ffi::OsString,
    fs,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::{
    core::{GUID, PCWSTR},
    Win32::{
        Foundation::*,
        Graphics::{
            Dwm::{DwmFlush, DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS},
            Gdi::*,
            GdiPlus::{
                GdipCreateBitmapFromScan0, GdipDisposeImage, GdipSaveImageToFile, GdiplusShutdown,
                GdiplusStartup, GdiplusStartupInput, GdiplusStartupOutput,
            },
        },
        System::{DataExchange::*, LibraryLoader::GetModuleHandleW, Memory::*},
        UI::{
            Controls::{
                Dialogs::{
                    GetSaveFileNameW, OFN_EXPLORER, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST,
                    OPENFILENAMEW,
                },
                SetWindowTheme, DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_COMBOBOXEDIT, ODS_SELECTED,
                WM_MOUSELEAVE,
            },
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            Shell::*,
            WindowsAndMessaging::*,
        },
    },
};

const MAIN: &str = "SnippingLiteMain";
const WINDOW_STYLE: u32 = WS_OVERLAPPEDWINDOW & !WS_THICKFRAME & !WS_MAXIMIZEBOX;
const OVERLAY: &str = "SnippingLiteOverlay";
const TOOLBAR: &str = "SnippingLiteToolbar";
const NEW: usize = 100;
const COPY: usize = 101;
const SAVE: usize = 102;
const DELAY: usize = 103;
const ASK: usize = 104;
const DELAYS: [u32; 4] = [0, 3, 5, 10];
const DELAY_TIMER: usize = 1;
const HOTKEY: i32 = 1;
const TRAY_MESSAGE: u32 = WM_APP + 1;
const TRAY_SETTINGS: usize = 201;
const TRAY_EXIT: usize = 202;
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
    start: POINT,
    end: POINT,
    points: Vec<POINT>,
    dragging: bool,
    return_visible: bool,
    copy: bool,
    save: bool,
    ask: bool,
    delay: u32,
    pending: bool,
    gdiplus: usize,
    settings_scale: f32,
    fonts: Option<Fonts>,
    card_brush: HBRUSH,
    field_brush: HBRUSH,
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
fn settings_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|x| PathBuf::from(x).join("Snipping Lite").join("settings.ini"))
}
fn parse_settings(text: &str) -> (bool, bool, bool, u32) {
    let delay = text
        .lines()
        .find_map(|s| s.strip_prefix("delay="))
        .and_then(|v| v.parse().ok())
        .filter(|d| DELAYS.contains(d))
        .unwrap_or(0);
    (
        !text.lines().any(|s| s == "copy=0"),
        text.lines().any(|s| s == "save=1"),
        text.lines().any(|s| s == "ask=1"),
        delay,
    )
}
fn load_settings() -> (bool, bool, bool, u32) {
    parse_settings(
        &settings_path()
            .and_then(|p| fs::read_to_string(p).ok())
            .unwrap_or_default(),
    )
}
fn save_settings(copy: bool, save: bool, ask: bool, delay: u32) -> std::io::Result<()> {
    let path = settings_path().ok_or_else(|| std::io::Error::other("APPDATA is unavailable"))?;
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| std::io::Error::other("Invalid settings path"))?,
    )?;
    fs::write(
        path,
        format!(
            "copy={}\r\nsave={}\r\nask={}\r\ndelay={}\r\n",
            copy as u8, save as u8, ask as u8, delay
        ),
    )
}
/// Creates a child control; `layout` positions and fonts it.
unsafe fn control(parent: HWND, class: &str, text: &str, id: usize, style: u32) -> HWND {
    CreateWindowExW(
        0,
        w(class).as_ptr(),
        w(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | (WS_TABSTOP * (class != "STATIC") as u32) | style,
        0,
        0,
        0,
        0,
        parent,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    )
}

// Settings window layout in DIPs (scaled by the monitor DPI).
const SETTINGS_WIDTH: f32 = 440.0;
const SETTINGS_HEIGHT: f32 = 424.0;
const CARD: [f32; 4] = [24.0, 168.0, 392.0, 192.0];
const ROW: f32 = 48.0;
const TITLE: usize = 110;
const SUBTITLE: usize = 111;
const HEADING: usize = 112;
const DELAY_LABEL: usize = 113;
const FOOTNOTE: usize = 114;
const STATUS: usize = 115;
#[derive(Clone, Copy)]
enum Face {
    Title,
    Heading,
    Body,
    Small,
}
/// Control id, rectangle (x, y, width, height) in DIPs and font.
const CONTROLS: [(usize, [f32; 4], Face); 10] = [
    (TITLE, [24.0, 20.0, 392.0, 30.0], Face::Title),
    (SUBTITLE, [24.0, 52.0, 392.0, 20.0], Face::Small),
    (NEW, [24.0, 84.0, 120.0, 32.0], Face::Body),
    (HEADING, [24.0, 136.0, 392.0, 24.0], Face::Heading),
    (COPY, [40.0, 180.0, 360.0, 24.0], Face::Body),
    (SAVE, [40.0, 228.0, 360.0, 24.0], Face::Body),
    (ASK, [40.0, 276.0, 360.0, 24.0], Face::Body),
    (DELAY_LABEL, [40.0, 324.0, 200.0, 24.0], Face::Body),
    (FOOTNOTE, [24.0, 372.0, 392.0, 18.0], Face::Small),
    (STATUS, [24.0, 392.0, 392.0, 18.0], Face::Small),
];
struct Fonts {
    title: ui::Font,
    heading: ui::Font,
    body: ui::Font,
    small: ui::Font,
}
impl Fonts {
    unsafe fn new(scale: f32) -> Fonts {
        Fonts {
            title: ui::Font::new(ui::TEXT_FONT, 20.0 * scale, FW_SEMIBOLD as i32),
            heading: ui::Font::new(ui::TEXT_FONT, 14.0 * scale, FW_SEMIBOLD as i32),
            body: ui::Font::new(ui::TEXT_FONT, 14.0 * scale, FW_NORMAL as i32),
            small: ui::Font::new(ui::TEXT_FONT, 12.0 * scale, FW_NORMAL as i32),
        }
    }
    fn get(&self, face: Face) -> HFONT {
        match face {
            Face::Title => self.title.0,
            Face::Heading => self.heading.0,
            Face::Body => self.body.0,
            Face::Small => self.small.0,
        }
    }
}
/// Positions every settings control for the current DPI and applies fonts.
unsafe fn layout(hwnd: HWND) {
    let scale = state().settings_scale;
    let fonts = Fonts::new(scale);
    let px = |v: f32| (v * scale).round() as i32;
    for (id, [x, y, width, height], face) in CONTROLS {
        let control = GetDlgItem(hwnd, id as i32);
        SetWindowPos(
            control,
            null_mut(),
            px(x),
            px(y),
            px(width),
            px(height),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        SendMessageW(control, WM_SETFONT, fonts.get(face) as WPARAM, 1);
    }
    // A combo box's height includes its drop-down list.
    let combo = GetDlgItem(hwnd, DELAY as i32);
    let field = px(32.0);
    let (left, right) = (CARD[0] + CARD[2] - 16.0 - 140.0, CARD[0] + CARD[2] - 16.0);
    SetWindowPos(
        combo,
        null_mut(),
        px(left),
        px(CARD[1] + 3.0 * ROW + 8.0),
        px(right - left),
        field + px(4.0 * 32.0 + 8.0),
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
    SendMessageW(combo, CB_SETITEMHEIGHT, -1isize as WPARAM, field as LPARAM);
    SendMessageW(combo, CB_SETITEMHEIGHT, 0, px(32.0) as LPARAM);
    SendMessageW(combo, WM_SETFONT, fonts.body.0 as WPARAM, 1);
    state().fonts = Some(fonts);
    InvalidateRect(hwnd, null(), 1);
}
/// Applies the light or dark theme to the window frame and controls.
unsafe fn apply_theme(hwnd: HWND) {
    state().theme = ui::theme();
    ui::window_theme(hwnd, &state().theme);
    // Check boxes sit on the card, so they are erased with its color.
    let card = state().theme.card;
    if !state().card_brush.is_null() {
        DeleteObject(state().card_brush);
    }
    if !state().field_brush.is_null() {
        DeleteObject(state().field_brush);
    }
    state().field_brush = CreateSolidBrush(if state().theme.dark {
        0x00333333
    } else {
        0x00ffffff
    });
    state().card_brush =
        CreateSolidBrush(((card >> 16) & 0xff) | (card & 0xff00) | ((card & 0xff) << 16));
    let dark = state().theme.dark;
    for (id, class) in [
        (NEW, "DarkMode_Explorer"),
        (COPY, "DarkMode_Explorer"),
        (SAVE, "DarkMode_Explorer"),
        (ASK, "DarkMode_Explorer"),
        (DELAY, "DarkMode_Explorer"),
    ] {
        let name = if dark { class } else { "Explorer" };
        SetWindowTheme(GetDlgItem(hwnd, id as i32), w(name).as_ptr(), null());
    }
    RedrawWindow(
        hwnd,
        null(),
        null_mut(),
        RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
    );
}
/// Paints the page background and the settings card into `dc`.
unsafe fn paint_settings(hwnd: HWND, dc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let Some(mut canvas) = ui::Canvas::new(client.right, client.bottom) else {
        return;
    };
    let s = state().settings_scale;
    let theme = &state().theme;
    canvas.fill_round_rect(
        [0.0, 0.0, client.right as f32, client.bottom as f32],
        0.0,
        theme.bg,
    );
    let [x, y, width, height] = CARD.map(|v| v * s);
    canvas.fill_round_rect([x, y, x + width, y + height], 8.0 * s, theme.stroke);
    let line = s.round().max(1.0);
    canvas.fill_round_rect(
        [x + line, y + line, x + width - line, y + height - line],
        7.0 * s,
        theme.card,
    );
    for row in 1..4 {
        let top = y + row as f32 * ROW * s;
        canvas.fill_round_rect(
            [x + line, top, x + width - line, top + line],
            0.0,
            theme.stroke,
        );
    }
    canvas.present(dc);
}
unsafe fn begin_capture() {
    if state().active || state().pending {
        return;
    }
    let owner = state().owner;
    state().return_visible = IsWindowVisible(owner) != 0;
    // Only wait for the compositor when one of our windows has to leave the screen first.
    let flush = state().return_visible || toast::visible();
    ShowWindow(owner, SW_HIDE);
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
        ShowWindow(owner, SW_SHOW);
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
        ShowWindow(owner, SW_SHOW);
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
const BAR_WIDTH: f32 = 2.0 * BAR_PADDING + 5.0 * BUTTON + 3.0 * BAR_GAP + DIVIDER;
const GLYPHS: [&str; 5] = ["\u{EF20}", "\u{EA49}", "\u{E7C4}", "\u{E7F4}", "\u{E711}"];
fn toolbar_button(index: i32, scale: f32) -> [f32; 4] {
    let mut left = BAR_PADDING + index as f32 * (BUTTON + BAR_GAP);
    if index == 4 {
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
fn screenshot_folder() -> std::io::Result<PathBuf> {
    let folder = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("USERPROFILE is unavailable"))?
        .join("Pictures")
        .join("Screenshots");
    fs::create_dir_all(&folder)?;
    Ok(folder)
}
unsafe fn choose_path(folder: &std::path::Path, name: &str, ask: bool) -> Option<PathBuf> {
    if !ask {
        return Some(folder.join(name));
    }
    let mut file = [0u16; 520];
    let initial: Vec<u16> = name.encode_utf16().collect();
    file[..initial.len()].copy_from_slice(&initial);
    let directory: Vec<u16> = folder.as_os_str().encode_wide().chain(Some(0)).collect();
    let filter: Vec<u16> = "PNG image\0*.png\0BMP image\0*.bmp\0\0"
        .encode_utf16()
        .collect();
    let extension = w("png");
    let mut dialog: OPENFILENAMEW = std::mem::zeroed();
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.nFilterIndex = 1;
    dialog.lpstrFile = file.as_mut_ptr();
    dialog.nMaxFile = file.len() as u32;
    dialog.lpstrInitialDir = directory.as_ptr();
    dialog.lpstrDefExt = extension.as_ptr();
    dialog.Flags = OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST;
    if GetSaveFileNameW(&mut dialog) == 0 {
        return None;
    }
    let length = file.iter().position(|&c| c == 0).unwrap_or(file.len());
    let path = PathBuf::from(OsString::from_wide(&file[..length]));
    Some(if path.extension().is_none() {
        path.with_extension(if dialog.nFilterIndex == 2 {
            "bmp"
        } else {
            "png"
        })
    } else {
        path
    })
}
unsafe fn save_image(
    pixels: &[u8],
    width: i32,
    height: i32,
    ask: bool,
) -> std::io::Result<Option<PathBuf>> {
    let folder = screenshot_folder()?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_millis();
    let Some(path) = choose_path(&folder, &format!("Snipping Lite {timestamp}.png"), ask) else {
        return Ok(None);
    };
    let bmp = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("bmp"));
    let encoder = GUID {
        data1: if bmp { 0x557cf400 } else { 0x557cf406 },
        data2: 0x1a04,
        data3: 0x11d3,
        data4: [0x9a, 0x73, 0x00, 0x00, 0xf8, 0x1e, 0xf3, 0x2e],
    };
    // GDI+ is started on first save; it is only needed for encoding.
    if state().gdiplus == 0 {
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            DebugEventCallback: 0,
            SuppressBackgroundThread: 0,
            SuppressExternalCodecs: 0,
        };
        let mut output: GdiplusStartupOutput = std::mem::zeroed();
        let mut token = 0;
        if GdiplusStartup(&mut token, &input, &mut output) != 0 {
            return Err(std::io::Error::other("Could not start GDI+"));
        }
        state().gdiplus = token;
    }
    let mut image = null_mut();
    // PixelFormat32bppARGB
    if GdipCreateBitmapFromScan0(
        width,
        height,
        width * 4,
        0x0026200A,
        pixels.as_ptr(),
        &mut image,
    ) != 0
    {
        return Err(std::io::Error::other("Could not create the image"));
    }
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let status = GdipSaveImageToFile(image.cast(), name.as_ptr(), &encoder, null());
    GdipDisposeImage(image.cast());
    if status == 0 {
        Ok(Some(path))
    } else {
        Err(std::io::Error::other("Could not encode the image"))
    }
}
unsafe fn copy_image(pixels: &[u8], width: i32, height: i32) -> bool {
    let header = std::mem::size_of::<BITMAPV5HEADER>();
    let global = GlobalAlloc(GMEM_MOVEABLE, header + pixels.len());
    if global.is_null() {
        return false;
    }
    let data = GlobalLock(global).cast::<u8>();
    if data.is_null() {
        GlobalFree(global);
        return false;
    }
    let mut info: BITMAPV5HEADER = std::mem::zeroed();
    info.bV5Size = header as u32;
    info.bV5Width = width;
    info.bV5Height = height;
    info.bV5Planes = 1;
    info.bV5BitCount = 32;
    info.bV5Compression = BI_BITFIELDS;
    info.bV5SizeImage = pixels.len() as u32;
    info.bV5RedMask = 0x00ff0000;
    info.bV5GreenMask = 0x0000ff00;
    info.bV5BlueMask = 0x000000ff;
    info.bV5AlphaMask = 0xff000000;
    info.bV5CSType = 0x73524742;
    info.bV5Intent = 4;
    std::ptr::copy_nonoverlapping((&info as *const BITMAPV5HEADER).cast::<u8>(), data, header);
    // Clipboard DIBs are bottom-up while the captured rows are top-down.
    let row = width as usize * 4;
    for (i, line) in pixels.chunks_exact(row).rev().enumerate() {
        std::ptr::copy_nonoverlapping(line.as_ptr(), data.add(header + i * row), row);
    }
    GlobalUnlock(global);
    let mut owned = false;
    if OpenClipboard(state().owner) != 0 {
        EmptyClipboard();
        owned = !SetClipboardData(17, global).is_null();
        CloseClipboard();
    }
    if !owned {
        GlobalFree(global);
    }
    owned
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
    let copied = state().copy && copy_image(&pixels, width, height);
    if state().copy && !copied {
        errors.push("Could not copy the image to the clipboard.");
    }
    if errors.is_empty() {
        toast::show(&pixels, width, height, copied, saved);
    } else {
        message(state().owner, &errors.join("\n"));
    }
}
unsafe fn cancel() {
    if state().active {
        ReleaseCapture();
        ShowWindow(state().toolbar, SW_HIDE);
        ShowWindow(state().overlay, SW_HIDE);
    }
    cleanup();
    if state().return_visible {
        ShowWindow(state().owner, SW_SHOW);
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
            if !free || state().points.len() >= 3 {
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
    let divider = toolbar_button(3, scale)[2] + (DIVIDER / 2.0 + BAR_GAP / 2.0 - 0.5) * scale;
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
                if hit == 4 {
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
unsafe fn store_settings(hwnd: HWND) {
    if save_settings(state().copy, state().save, state().ask, state().delay).is_err() {
        message(hwnd, "Could not save the settings.");
    }
}
unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            state().owner = hwnd;
            state().settings_scale = GetDpiForWindow(hwnd) as f32 / 96.0;
            ui::disable_transitions(hwnd);
            tray(hwnd, NIM_ADD);
            control(hwnd, "STATIC", "Snipping Lite", TITLE, 0);
            control(
                hwnd,
                "STATIC",
                "Capture screenshots with Alt + Shift + S",
                SUBTITLE,
                0,
            );
            control(hwnd, "BUTTON", "New snip", NEW, BS_PUSHBUTTON as u32);
            control(hwnd, "STATIC", "Settings", HEADING, 0);
            let checkbox = BS_AUTOCHECKBOX as u32;
            let copy = control(
                hwnd,
                "BUTTON",
                "Automatically copy screenshots",
                COPY,
                checkbox,
            );
            let save = control(
                hwnd,
                "BUTTON",
                "Automatically save screenshots",
                SAVE,
                checkbox,
            );
            let ask = control(
                hwnd,
                "BUTTON",
                "Choose a file name when saving",
                ASK,
                checkbox,
            );
            SendMessageW(copy, BM_SETCHECK, if state().copy { 1 } else { 0 }, 0);
            SendMessageW(save, BM_SETCHECK, if state().save { 1 } else { 0 }, 0);
            SendMessageW(ask, BM_SETCHECK, if state().ask { 1 } else { 0 }, 0);
            control(
                hwnd,
                "STATIC",
                "Capture delay",
                DELAY_LABEL,
                0x200, // SS_CENTERIMAGE
            );
            let delay = control(
                hwnd,
                "COMBOBOX",
                "",
                DELAY,
                WS_VSCROLL | (CBS_DROPDOWNLIST | CBS_OWNERDRAWFIXED | CBS_HASSTRINGS) as u32,
            );
            for label in ["None", "3 seconds", "5 seconds", "10 seconds"] {
                SendMessageW(delay, CB_ADDSTRING, 0, w(label).as_ptr() as LPARAM);
            }
            let selected = DELAYS.iter().position(|&d| d == state().delay).unwrap_or(0);
            SendMessageW(delay, CB_SETCURSEL, selected, 0);
            control(
                hwnd,
                "STATIC",
                "Saved images: Pictures\\Screenshots (PNG or BMP)",
                FOOTNOTE,
                0,
            );
            let status = if RegisterHotKey(
                hwnd,
                HOTKEY,
                MOD_ALT | MOD_SHIFT | MOD_NOREPEAT,
                b'S' as u32,
            ) != 0
            {
                "Shortcut: Alt + Shift + S"
            } else {
                "Shortcut unavailable: another application is using it."
            };
            control(hwnd, "STATIC", status, STATUS, 0);
            layout(hwnd);
            apply_theme(hwnd);
            0
        }
        WM_ERASEBKGND => {
            paint_settings(hwnd, wp as HDC);
            1
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let dc = wp as HDC;
            let id = GetDlgCtrlID(lp as HWND) as usize;
            let secondary = [SUBTITLE, FOOTNOTE, STATUS].contains(&id);
            let theme = &state().theme;
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(
                dc,
                if secondary {
                    theme.secondary_ref
                } else {
                    theme.text_ref
                },
            );
            // Themed check boxes ask for their background with WM_CTLCOLORSTATIC.
            if [COPY, SAVE, ASK].contains(&id) {
                state().card_brush as LRESULT
            } else {
                GetStockObject(NULL_BRUSH) as LRESULT
            }
        }
        WM_MEASUREITEM => {
            let item = &mut *(lp as *mut MEASUREITEMSTRUCT);
            item.itemHeight = (32.0 * state().settings_scale).round() as u32;
            1
        }
        WM_DRAWITEM => {
            let item = &*(lp as *const DRAWITEMSTRUCT);
            if item.CtlID as usize != DELAY || (item.itemID as i32) < 0 {
                return 0;
            }
            let theme = &state().theme;
            let selected =
                item.itemState & ODS_SELECTED != 0 && item.itemState & ODS_COMBOBOXEDIT == 0;
            let field = if theme.dark { 0x00333333 } else { 0x00ffffff };
            let brush = CreateSolidBrush(if selected {
                if theme.dark {
                    0x00454545
                } else {
                    0x00f0f0f0
                }
            } else {
                field
            });
            FillRect(item.hDC, &item.rcItem, brush);
            DeleteObject(brush);
            let mut label = [0u16; 32];
            let length = SendMessageW(
                item.hwndItem,
                CB_GETLBTEXT,
                item.itemID as WPARAM,
                label.as_mut_ptr() as LPARAM,
            );
            let mut area = item.rcItem;
            area.left += (10.0 * state().settings_scale) as i32;
            SetBkMode(item.hDC, TRANSPARENT as i32);
            SetTextColor(item.hDC, theme.text_ref);
            DrawTextW(
                item.hDC,
                label.as_mut_ptr(),
                length.max(0) as i32,
                &mut area,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            1
        }
        WM_CTLCOLORLISTBOX | WM_CTLCOLOREDIT => {
            let dc = wp as HDC;
            let dark = state().theme.dark;
            SetTextColor(dc, state().theme.text_ref);
            SetBkColor(dc, if dark { 0x00333333 } else { 0x00ffffff });
            state().field_brush as LRESULT
        }
        WM_DPICHANGED => {
            state().settings_scale = ((wp >> 16) & 0xffff) as f32 / 96.0;
            let r = &*(lp as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            layout(hwnd);
            0
        }
        WM_SETTINGCHANGE => {
            apply_theme(hwnd);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_COMMAND => {
            match wp & 0xffff {
                NEW => begin_capture(),
                TRAY_SETTINGS => {
                    ShowWindow(hwnd, SW_SHOW);
                    SetForegroundWindow(hwnd);
                }
                TRAY_EXIT => {
                    DestroyWindow(hwnd);
                }
                DELAY if (wp >> 16) as u32 == CBN_SELCHANGE => {
                    let index = SendMessageW(GetDlgItem(hwnd, DELAY as i32), CB_GETCURSEL, 0, 0);
                    if let Some(&delay) = DELAYS.get(index as usize) {
                        state().delay = delay;
                    }
                    store_settings(hwnd);
                }
                COPY | SAVE | ASK => {
                    let copy = SendMessageW(GetDlgItem(hwnd, COPY as i32), BM_GETCHECK, 0, 0) == 1;
                    let save = SendMessageW(GetDlgItem(hwnd, SAVE as i32), BM_GETCHECK, 0, 0) == 1;
                    if !copy && !save {
                        message(hwnd, "Choose at least one destination for screenshots.");
                        SendMessageW(GetDlgItem(hwnd, COPY as i32), BM_SETCHECK, 1, 0);
                        state().copy = true;
                        state().save = false;
                    } else {
                        state().copy = copy;
                        state().save = save;
                    }
                    state().ask =
                        SendMessageW(GetDlgItem(hwnd, ASK as i32), BM_GETCHECK, 0, 0) == 1;
                    store_settings(hwnd);
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
                AppendMenuW(menu, MF_STRING, NEW, w("New snip").as_ptr());
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
    data.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
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
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let (copy, save, ask, delay) = load_settings();
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
            return_visible: true,
            copy,
            save,
            ask,
            delay,
            pending: false,
            gdiplus: 0,
            settings_scale: 1.0,
            fonts: None,
            card_brush: null_mut(),
            field_brush: null_mut(),
        });
        STATE = &mut *app;
        register(MAIN, Some(main_proc), null_mut(), IDC_ARROW);
        register(OVERLAY, Some(overlay_proc), null_mut(), IDC_CROSS);
        register(TOOLBAR, Some(toolbar_proc), null_mut(), IDC_ARROW);
        register(toast::CLASS, Some(toast::proc), null_mut(), IDC_HAND);
        let dpi = GetDpiForSystem();
        let mut frame = RECT {
            left: 0,
            top: 0,
            right: (SETTINGS_WIDTH * dpi as f32 / 96.0).round() as i32,
            bottom: (SETTINGS_HEIGHT * dpi as f32 / 96.0).round() as i32,
        };
        AdjustWindowRectExForDpi(&mut frame, WINDOW_STYLE, 0, 0, dpi);
        let hwnd = CreateWindowExW(
            0,
            w(MAIN).as_ptr(),
            w("Snipping Lite").as_ptr(),
            WINDOW_STYLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            frame.right - frame.left,
            frame.bottom - frame.top,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        create_windows();
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if state().gdiplus != 0 {
            GdiplusShutdown(state().gdiplus);
        }
        STATE = null_mut();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_fall_back_to_defaults() {
        assert_eq!(parse_settings(""), (true, false, false, 0));
        assert_eq!(
            parse_settings("copy=0\r\nsave=1\r\nask=1\r\ndelay=5\r\n"),
            (false, true, true, 5)
        );
        assert_eq!(parse_settings("delay=7").3, 0);
    }
    #[test]
    fn selection_bounds_are_sorted_and_clipped() {
        let r = bounds(POINT { x: 140, y: 40 }, POINT { x: -10, y: 10 }, 100, 100);
        assert_eq!((r.left, r.top, r.right, r.bottom), (0, 10, 100, 40));
    }
}
