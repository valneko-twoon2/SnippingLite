use super::*;
use std::{ptr::null, sync::atomic::AtomicUsize};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::{Dwm::DwmFlush, Gdi::*},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        HiDpi::{
            SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        },
        WindowsAndMessaging::*,
    },
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
pub const GREEN: u32 = 0x0020c040;
pub const RED: u32 = 0x002040e0;
pub const BLUE: u32 = 0x00e06020;

unsafe extern "system" fn paint(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_PAINT {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut ps);
        let brush = CreateSolidBrush(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u32);
        if !brush.is_null() {
            FillRect(dc, &ps.rcPaint, brush);
            DeleteObject(brush);
        }
        EndPaint(hwnd, &ps);
        0
    } else {
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}

pub struct CoveredWindow {
    pub target: HWND,
    cover: HWND,
    class: Vec<u16>,
    dpi: DPI_AWARENESS_CONTEXT,
}
impl CoveredWindow {
    pub fn new() -> Self {
        // SAFETY: these are owned test windows, painted on this test thread.
        unsafe {
            let dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let class = crate::w(&format!(
                "SnippingLiteCaptureTest{}",
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let spec = WNDCLASSW {
                lpfnWndProc: Some(paint),
                hInstance: GetModuleHandleW(null()),
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            assert_ne!(RegisterClassW(&spec), 0);
            let create = |color, style| {
                let hwnd = CreateWindowExW(
                    // CI の Windows Server 2025 では、窓の撮影は、拡張スタイルのない普通の窓しか受け付けない
                    // （WS_EX_TOOLWINDOW も WS_EX_NOACTIVATE も拒まれた）ので、撮る窓には付けない。
                    // 表示は SW_SHOWNOACTIVATE なので、前面は奪わない。
                    0,
                    class.as_ptr(),
                    crate::w("Snipping Lite capture validation").as_ptr(),
                    style,
                    80,
                    80,
                    320,
                    240,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    GetModuleHandleW(null()),
                    null(),
                );
                assert!(!hwnd.is_null());
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, color as isize);
                ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                UpdateWindow(hwnd);
                hwnd
            };
            let target = create(GREEN, WS_OVERLAPPEDWINDOW);
            let cover = create(RED, WS_POPUP);
            SetWindowPos(cover, HWND_TOPMOST, 80, 80, 320, 240, SWP_NOACTIVATE);
            DwmFlush();
            Self {
                target,
                cover,
                class,
                dpi,
            }
        }
    }

    pub fn recolor(&self, color: u32) {
        unsafe {
            SetWindowLongPtrW(self.target, GWLP_USERDATA, color as isize);
            InvalidateRect(self.target, null(), 1);
            UpdateWindow(self.target);
            DwmFlush();
        }
    }

    pub fn desktop_is_red(&self) -> bool {
        unsafe {
            let dc = GetDC(std::ptr::null_mut());
            let color = GetPixel(dc, 200, 200);
            ReleaseDC(std::ptr::null_mut(), dc);
            color & 0xff > ((color >> 8) & 0xff) + 60
        }
    }
}
impl Drop for CoveredWindow {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.cover);
            DestroyWindow(self.target);
            UnregisterClassW(self.class.as_ptr(), GetModuleHandleW(null()));
            if !self.dpi.is_null() {
                SetThreadDpiAwarenessContext(self.dpi);
            }
        }
    }
}

/// 枠も題名もない窓（WS_POPUP）。隠したまま作るので、画面には出ない。壊すのは呼んだ側。
pub fn popup(x: i32, y: i32, width: i32, height: i32) -> HWND {
    // SAFETY: a private window of the system STATIC class, owned by the calling test.
    unsafe {
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            crate::w("STATIC").as_ptr(),
            null(),
            WS_POPUP,
            x,
            y,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        assert!(!hwnd.is_null());
        hwnd
    }
}

pub fn center(pixels: &[u8], width: i32, height: i32) -> [u8; 4] {
    let offset = ((height / 2 * width + width / 2) * 4) as usize;
    [
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
        pixels[offset + 3],
    ]
}
