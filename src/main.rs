#![windows_subsystem = "windows"]

use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::DataExchange::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const MAIN: &str = "SnippingLiteMain";
const OVERLAY: &str = "SnippingLiteOverlay";
const CAPTURE: usize = 100;
const HOTKEY: i32 = 1;
static mut START: POINT = POINT { x: 0, y: 0 };
static mut END: POINT = POINT { x: 0, y: 0 };
static mut DRAGGING: bool = false;
static mut OWNER: HWND = null_mut();

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
unsafe fn label(hwnd: HWND, text: &str) {
    SetWindowTextW(hwnd, wide(text).as_ptr());
}
unsafe fn message(hwnd: HWND, text: &str) {
    MessageBoxW(
        hwnd,
        wide(text).as_ptr(),
        wide("Snipping Lite").as_ptr(),
        MB_OK | MB_ICONINFORMATION,
    );
}

unsafe fn copy_rectangle(x: i32, y: i32, width: i32, height: i32) -> bool {
    if width <= 0 || height <= 0 {
        return false;
    }
    let screen = GetDC(null_mut());
    if screen.is_null() {
        return false;
    }
    let memory = CreateCompatibleDC(screen);
    let bitmap = CreateCompatibleBitmap(screen, width, height);
    if memory.is_null() || bitmap.is_null() {
        if !memory.is_null() {
            DeleteDC(memory);
        }
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        ReleaseDC(null_mut(), screen);
        return false;
    }
    let old = SelectObject(memory, bitmap);
    let copied = BitBlt(
        memory,
        0,
        0,
        width,
        height,
        screen,
        x,
        y,
        SRCCOPY | CAPTUREBLT,
    ) != 0;
    SelectObject(memory, old);
    DeleteDC(memory);
    ReleaseDC(null_mut(), screen);
    if copied && OpenClipboard(null_mut()) != 0 {
        EmptyClipboard();
        let success = !SetClipboardData(2, bitmap).is_null();
        CloseClipboard();
        if success {
            return true;
        }
    }
    DeleteObject(bitmap);
    false
}

unsafe fn begin_capture(owner: HWND) {
    ShowWindow(owner, SW_HIDE);
    let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
    let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
    let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
    let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
    let instance = GetModuleHandleW(null());
    let overlay = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
        wide(OVERLAY).as_ptr(),
        wide("Select an area — Esc to cancel").as_ptr(),
        WS_POPUP,
        x,
        y,
        w,
        h,
        null_mut(),
        null_mut(),
        instance,
        null(),
    );
    if overlay.is_null() {
        ShowWindow(owner, SW_SHOW);
        return;
    }
    SetLayeredWindowAttributes(overlay, 0, 90, LWA_ALPHA);
    ShowWindow(overlay, SW_SHOW);
    SetForegroundWindow(overlay);
    SetFocus(overlay);
}
unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            START = POINT {
                x: (lp as u32 as u16 as i16) as i32,
                y: ((lp as u32 >> 16) as u16 as i16) as i32,
            };
            END = START;
            DRAGGING = true;
            SetCapture(hwnd);
            0
        }
        WM_MOUSEMOVE if DRAGGING => {
            END = POINT {
                x: (lp as u32 as u16 as i16) as i32,
                y: ((lp as u32 >> 16) as u16 as i16) as i32,
            };
            InvalidateRect(hwnd, null(), 1);
            0
        }
        WM_LBUTTONUP if DRAGGING => {
            END = POINT {
                x: (lp as u32 as u16 as i16) as i32,
                y: ((lp as u32 >> 16) as u16 as i16) as i32,
            };
            DRAGGING = false;
            ReleaseCapture();
            let left = START.x.min(END.x) + GetSystemMetrics(SM_XVIRTUALSCREEN);
            let top = START.y.min(END.y) + GetSystemMetrics(SM_YVIRTUALSCREEN);
            let width = (START.x - END.x).abs();
            let height = (START.y - END.y).abs();
            ShowWindow(hwnd, SW_HIDE);
            DestroyWindow(hwnd);
            if width > 0 && height > 0 && !copy_rectangle(left, top, width, height) {
                message(OWNER, "Could not copy the capture to the clipboard.");
            }
            ShowWindow(OWNER, SW_SHOW);
            0
        }
        WM_KEYDOWN if wp as u32 == VK_ESCAPE as u32 => {
            DRAGGING = false;
            DestroyWindow(hwnd);
            ShowWindow(OWNER, SW_SHOW);
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            FillRect(dc, &rect, GetStockObject(BLACK_BRUSH) as HBRUSH);
            if DRAGGING {
                let pen = CreatePen(PS_SOLID, 3, 0x00ffffff);
                let oldpen = SelectObject(dc, pen);
                let oldbrush = SelectObject(dc, GetStockObject(NULL_BRUSH));
                Rectangle(
                    dc,
                    START.x.min(END.x),
                    START.y.min(END.y),
                    START.x.max(END.x),
                    START.y.max(END.y),
                );
                SelectObject(dc, oldbrush);
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
            OWNER = hwnd;
            let instance = GetModuleHandleW(null());
            CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide("Capture a rectangular area and copy it to the clipboard.").as_ptr(),
                WS_CHILD | WS_VISIBLE,
                20,
                20,
                420,
                25,
                hwnd,
                null_mut(),
                instance,
                null(),
            );
            CreateWindowExW(
                0,
                wide("BUTTON").as_ptr(),
                wide("New capture").as_ptr(),
                WS_CHILD | WS_VISIBLE | BS_PUSHBUTTON as u32,
                20,
                55,
                150,
                35,
                hwnd,
                CAPTURE as HMENU,
                instance,
                null(),
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
                "Alt + Shift + S is already in use. Use New capture."
            };
            CreateWindowExW(
                0,
                wide("STATIC").as_ptr(),
                wide(status).as_ptr(),
                WS_CHILD | WS_VISIBLE,
                20,
                110,
                420,
                45,
                hwnd,
                null_mut(),
                instance,
                null(),
            );
            0
        }
        WM_COMMAND if (wp & 0xffff) == CAPTURE => {
            begin_capture(hwnd);
            0
        }
        WM_HOTKEY if wp == HOTKEY as usize => {
            begin_capture(hwnd);
            0
        }
        WM_DESTROY => {
            UnregisterHotKey(hwnd, HOTKEY);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}
unsafe fn register(name: &str, proc: WNDPROC, brush: HBRUSH) {
    let instance = GetModuleHandleW(null());
    let class_name = wide(name);
    let class = WNDCLASSW {
        lpfnWndProc: proc,
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_CROSS),
        hbrBackground: brush,
        ..std::mem::zeroed()
    };
    RegisterClassW(&class);
}
fn main() {
    unsafe {
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
        register(MAIN, Some(main_proc), (COLOR_WINDOW + 1) as HBRUSH);
        register(
            OVERLAY,
            Some(overlay_proc),
            GetStockObject(BLACK_BRUSH) as HBRUSH,
        );
        let hwnd = CreateWindowExW(
            0,
            wide(MAIN).as_ptr(),
            wide("Snipping Lite").as_ptr(),
            WS_OVERLAPPEDWINDOW & !WS_THICKFRAME & !WS_MAXIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            480,
            210,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        label(hwnd, "Snipping Lite");
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
