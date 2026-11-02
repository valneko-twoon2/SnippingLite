//! 窓が撮れなかったとき（`CreateForWindow` が拒んだとき）に、窓の状態を 1 行にまとめる。
//! 手元では撮れる窓が CI で拒まれる理由（隠れている、cloaked、ツール窓、別のセッション）を、
//! 標準エラーの出力から読めるようにするためのもの。

use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED},
    UI::WindowsAndMessaging::{
        GetAncestor, GetClassNameW, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
        IsIconic, IsWindow, IsWindowVisible, GA_ROOT, GWL_EXSTYLE, GWL_STYLE,
    },
};

pub(super) fn describe(id: usize) -> String {
    let hwnd = id as HWND;
    // SAFETY: plain queries; an invalid handle only makes them fail.
    unsafe {
        let mut class = [0u16; 64];
        let length = GetClassNameW(hwnd, class.as_mut_ptr(), 64).clamp(0, 64) as usize;
        let mut cloaked = 0u32;
        let cloak = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED as u32,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let mut rect: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut rect);
        let mut process = 0;
        let thread = GetWindowThreadProcessId(hwnd, &mut process);
        format!(
            "window {id:#x} class {:?} valid={} visible={} iconic={} root={:#x} style={:#x} \
             exstyle={:#x} cloaked={cloaked} (hr {cloak:#x}) rect=({}, {}, {}, {}) \
             process={process} (this {}) thread={thread}",
            String::from_utf16_lossy(&class[..length]),
            IsWindow(hwnd) != 0,
            IsWindowVisible(hwnd) != 0,
            IsIconic(hwnd) != 0,
            GetAncestor(hwnd, GA_ROOT) as usize,
            GetWindowLongPtrW(hwnd, GWL_STYLE),
            GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            std::process::id(),
        )
    }
}
