#![windows_subsystem = "windows"]

use std::{
    fs,
    io::Write,
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::DwmFlush, Gdi::*},
    System::{DataExchange::*, LibraryLoader::GetModuleHandleW},
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

const MAIN: &str = "SnippingLiteMain";
const OVERLAY: &str = "SnippingLiteOverlay";
const NEW: usize = 100;
const COPY: usize = 101;
const SAVE: usize = 102;
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
fn load_settings() -> (bool, bool) {
    let text = settings_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_default();
    (
        !text.lines().any(|s| s == "copy=0"),
        text.lines().any(|s| s == "save=1"),
    )
}
fn save_settings(copy: bool, save: bool) -> std::io::Result<()> {
    let path = settings_path().ok_or_else(|| std::io::Error::other("APPDATA is unavailable"))?;
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| std::io::Error::other("Invalid settings path"))?,
    )?;
    fs::write(
        path,
        format!("copy={}\r\nsave={}\r\n", copy as u8, save as u8),
    )
}
unsafe fn button(
    parent: HWND,
    text: &str,
    id: usize,
    x: i32,
    y: i32,
    width: i32,
    style: u32,
) -> HWND {
    CreateWindowExW(
        0,
        w("BUTTON").as_ptr(),
        w(text).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        x,
        y,
        width,
        30,
        parent,
        id as HMENU,
        GetModuleHandleW(null()),
        null(),
    )
}
unsafe fn text(parent: HWND, value: &str, x: i32, y: i32, width: i32) {
    CreateWindowExW(
        0,
        w("STATIC").as_ptr(),
        w(value).as_ptr(),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        width,
        36,
        parent,
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
}
unsafe fn begin_capture() {
    if !state().overlay.is_null() {
        return;
    }
    let owner = state().owner;
    state().return_visible = IsWindowVisible(owner) != 0;
    ShowWindow(owner, SW_HIDE);
    DwmFlush();
    let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    let dc = GetDC(null_mut());
    let mem = CreateCompatibleDC(dc);
    let bitmap = CreateCompatibleBitmap(dc, width, height);
    if dc.is_null() || mem.is_null() || bitmap.is_null() {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !mem.is_null() {
            DeleteDC(mem);
        }
        if !dc.is_null() {
            ReleaseDC(null_mut(), dc);
        }
        ShowWindow(owner, SW_SHOW);
        message(owner, "Could not capture the desktop.");
        return;
    }
    let old = SelectObject(mem, bitmap);
    let captured = BitBlt(mem, 0, 0, width, height, dc, x, y, SRCCOPY | CAPTUREBLT) != 0;
    ReleaseDC(null_mut(), dc);
    if !captured {
        SelectObject(mem, old);
        DeleteDC(mem);
        DeleteObject(bitmap);
        ShowWindow(owner, SW_SHOW);
        message(owner, "Could not capture the desktop.");
        return;
    }
    state().origin = POINT { x, y };
    state().width = width;
    state().height = height;
    state().screen = bitmap;
    state().screen_dc = mem;
    state().previous = old;
    state().mode = MODE_RECT;
    state().points.clear();
    let overlay = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
        w(OVERLAY).as_ptr(),
        w("Snipping Lite").as_ptr(),
        WS_POPUP,
        x,
        y,
        width,
        height,
        null_mut(),
        null_mut(),
        GetModuleHandleW(null()),
        null(),
    );
    if overlay.is_null() {
        cleanup();
        ShowWindow(owner, SW_SHOW);
        return;
    }
    state().overlay = overlay;
    SetLayeredWindowAttributes(overlay, 0x00ff00ff, 175, LWA_ALPHA | LWA_COLORKEY);
    ShowWindow(overlay, SW_SHOW);
    SetForegroundWindow(overlay);
    SetFocus(overlay);
}
unsafe fn cleanup() {
    if !state().screen_dc.is_null() {
        SelectObject(state().screen_dc, state().previous);
        DeleteDC(state().screen_dc);
        state().screen_dc = null_mut();
    }
    if !state().screen.is_null() {
        DeleteObject(state().screen);
        state().screen = null_mut();
    }
    state().overlay = null_mut();
    state().dragging = false;
    state().points.clear();
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
unsafe fn toolbar() -> (i32, i32) {
    (
        (GetSystemMetrics(SM_CXSCREEN) - 520) / 2 - state().origin.x,
        30 - state().origin.y,
    )
}
unsafe fn select_window(p: POINT) -> Option<RECT> {
    let absolute = POINT {
        x: p.x + state().origin.x,
        y: p.y + state().origin.y,
    };
    let mut window = GetWindow(state().overlay, GW_HWNDNEXT);
    while !window.is_null() {
        let mut rect: RECT = std::mem::zeroed();
        if window != state().owner
            && IsWindowVisible(window) != 0
            && IsIconic(window) == 0
            && GetWindowRect(window, &mut rect) != 0
            && absolute.x >= rect.left
            && absolute.x < rect.right
            && absolute.y >= rect.top
            && absolute.y < rect.bottom
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
unsafe fn save_bitmap(bitmap: HBITMAP, width: i32, height: i32) -> std::io::Result<PathBuf> {
    let row = (width as usize)
        .checked_mul(4)
        .ok_or_else(|| std::io::Error::other("Image too large"))?;
    let len = row
        .checked_mul(height as usize)
        .ok_or_else(|| std::io::Error::other("Image too large"))?;
    let mut pixels = vec![0u8; len];
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let dc = GetDC(null_mut());
    let ok = GetDIBits(
        dc,
        bitmap,
        0,
        height as u32,
        pixels.as_mut_ptr().cast(),
        &mut info,
        DIB_RGB_COLORS,
    );
    ReleaseDC(null_mut(), dc);
    if ok == 0 {
        return Err(std::io::Error::other("Could not read bitmap"));
    }
    let folder = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("USERPROFILE is unavailable"))?
        .join("Pictures")
        .join("Screenshots");
    fs::create_dir_all(&folder)?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_millis();
    let path = folder.join(format!("Snipping Lite {timestamp}.bmp"));
    let mut file = fs::File::create(&path)?;
    let total = 54u32
        .checked_add(
            len.try_into()
                .map_err(|_| std::io::Error::other("Image too large"))?,
        )
        .ok_or_else(|| std::io::Error::other("Image too large"))?;
    file.write_all(b"BM")?;
    file.write_all(&total.to_le_bytes())?;
    file.write_all(&[0; 4])?;
    file.write_all(&54u32.to_le_bytes())?;
    file.write_all(&40u32.to_le_bytes())?;
    file.write_all(&width.to_le_bytes())?;
    file.write_all(&height.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&32u16.to_le_bytes())?;
    file.write_all(&[0; 24])?;
    file.write_all(&pixels)?;
    Ok(path)
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
    if freeform && state().points.len() >= 3 {
        let points: Vec<POINT> = state()
            .points
            .iter()
            .map(|p| POINT {
                x: p.x - rect.left,
                y: p.y - rect.top,
            })
            .collect();
        PatBlt(mem, 0, 0, width, height, WHITENESS);
        let region = CreatePolygonRgn(points.as_ptr(), points.len() as i32, ALTERNATE);
        SelectClipRgn(mem, region);
        DeleteObject(region);
    }
    let ok = BitBlt(
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
    DeleteDC(mem);
    let mut errors = Vec::new();
    if !ok {
        errors.push("Could not copy the selected pixels.");
    }
    if ok && state().save && save_bitmap(bitmap, width, height).is_err() {
        errors.push("Could not save the image in Pictures\\Screenshots.");
    }
    let mut owned = false;
    if ok && state().copy {
        if OpenClipboard(state().owner) != 0 {
            EmptyClipboard();
            owned = !SetClipboardData(2, bitmap).is_null();
            CloseClipboard();
        }
        if !owned {
            errors.push("Could not copy the image to the clipboard.");
        }
    }
    if !owned {
        DeleteObject(bitmap);
    }
    cancel();
    if !errors.is_empty() {
        message(state().owner, &errors.join("\n"));
    }
}
unsafe fn cancel() {
    let overlay = state().overlay;
    if !overlay.is_null() {
        DestroyWindow(overlay);
    }
    cleanup();
    if state().return_visible {
        ShowWindow(state().owner, SW_SHOW);
    }
}
unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            let p = point(lp);
            let (tx, ty) = toolbar();
            if p.y >= ty && p.y < ty + 52 && p.x >= tx && p.x < tx + 520 {
                let index = (p.x - tx) / 104;
                if index == 4 {
                    cancel();
                    return 0;
                }
                state().mode = index;
                InvalidateRect(hwnd, null(), 1);
                if index == MODE_SCREEN {
                    finish(
                        RECT {
                            left: 0,
                            top: 0,
                            right: state().width,
                            bottom: state().height,
                        },
                        false,
                    );
                }
                return 0;
            }
            if state().mode == MODE_WINDOW {
                if let Some(rect) = select_window(p) {
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
            state().end = p;
            if state().mode == MODE_FREE {
                state().points.push(p);
            }
            InvalidateRect(hwnd, null(), 1);
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
            }
            0
        }
        WM_KEYDOWN if wp as u32 == VK_ESCAPE as u32 => {
            cancel();
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            FillRect(dc, &rect, GetStockObject(BLACK_BRUSH) as HBRUSH);
            let (x, y) = toolbar();
            let brush = CreateSolidBrush(0x00333333);
            let bar = RECT {
                left: x,
                top: y,
                right: x + 520,
                bottom: y + 52,
            };
            FillRect(dc, &bar, brush);
            DeleteObject(brush);
            SetBkMode(dc, TRANSPARENT as i32);
            SetTextColor(dc, 0x00ffffff);
            for (i, title) in ["Rectangle", "Freeform", "Window", "Full screen", "Close"]
                .iter()
                .enumerate()
            {
                let mut r = RECT {
                    left: x + (i as i32) * 104,
                    top: y + 8,
                    right: x + (i as i32 + 1) * 104,
                    bottom: y + 44,
                };
                if i as i32 == state().mode {
                    let b = CreateSolidBrush(0x008b5a21);
                    FillRect(dc, &r, b);
                    DeleteObject(b);
                }
                let mut s = w(title);
                DrawTextW(
                    dc,
                    s.as_mut_ptr(),
                    -1,
                    &mut r,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            }
            if state().dragging {
                let pen = CreatePen(PS_SOLID, 3, 0x00ffffff);
                let oldpen = SelectObject(dc, pen);
                let clear = CreateSolidBrush(0x00ff00ff);
                let oldbrush = SelectObject(dc, clear);
                if state().mode == MODE_FREE && state().points.len() > 1 {
                    Polygon(dc, state().points.as_ptr(), state().points.len() as i32);
                } else {
                    Rectangle(
                        dc,
                        state().start.x.min(state().end.x),
                        state().start.y.min(state().end.y),
                        state().start.x.max(state().end.x),
                        state().start.y.max(state().end.y),
                    );
                }
                SelectObject(dc, oldbrush);
                DeleteObject(clear);
                SelectObject(dc, oldpen);
                DeleteObject(pen);
            }
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            state().owner = hwnd;
            tray(hwnd, NIM_ADD);
            text(
                hwnd,
                "Capture screenshots with Alt + Shift + S",
                24,
                20,
                430,
            );
            button(hwnd, "New snip", NEW, 24, 58, 150, BS_PUSHBUTTON as u32);
            text(hwnd, "Settings", 24, 110, 430);
            let copy = button(
                hwnd,
                "Automatically copy screenshots",
                COPY,
                24,
                145,
                350,
                BS_AUTOCHECKBOX as u32,
            );
            let save = button(
                hwnd,
                "Automatically save screenshots",
                SAVE,
                24,
                182,
                350,
                BS_AUTOCHECKBOX as u32,
            );
            SendMessageW(copy, BM_SETCHECK, if state().copy { 1 } else { 0 }, 0);
            SendMessageW(save, BM_SETCHECK, if state().save { 1 } else { 0 }, 0);
            text(
                hwnd,
                "Saved images: Pictures\\Screenshots (BMP)",
                24,
                220,
                430,
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
            text(hwnd, status, 24, 260, 430);
            0
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
                COPY | SAVE => {
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
                    if save_settings(state().copy, state().save).is_err() {
                        message(hwnd, "Could not save the settings.");
                    }
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
        WM_HOTKEY if wp == HOTKEY as usize => {
            begin_capture();
            0
        }
        WM_DESTROY => {
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
unsafe fn register(name: &str, proc: WNDPROC, brush: HBRUSH) {
    let class_name = w(name);
    let class = WNDCLASSW {
        lpfnWndProc: proc,
        hInstance: GetModuleHandleW(null()),
        lpszClassName: class_name.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_CROSS),
        hbrBackground: brush,
        ..std::mem::zeroed()
    };
    RegisterClassW(&class);
}
fn main() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let (copy, save) = load_settings();
        let mut app = Box::new(State {
            owner: null_mut(),
            overlay: null_mut(),
            screen: null_mut(),
            screen_dc: null_mut(),
            previous: null_mut(),
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
        });
        STATE = &mut *app;
        register(MAIN, Some(main_proc), (COLOR_WINDOW + 1) as HBRUSH);
        register(
            OVERLAY,
            Some(overlay_proc),
            GetStockObject(BLACK_BRUSH) as HBRUSH,
        );
        let hwnd = CreateWindowExW(
            0,
            w(MAIN).as_ptr(),
            w("Snipping Lite").as_ptr(),
            WS_OVERLAPPEDWINDOW & !WS_THICKFRAME & !WS_MAXIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            480,
            360,
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
