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

/// 撮れなかった窓のほかに、普通の窓、ツール窓、画面全体なら撮れるかを試して、結果を 1 行にまとめる。
/// 窓の状態に問題がないのに拒まれるとき、拒む条件（窓の種類か、撮影の機能そのものか）を切り分ける。
pub(super) fn probe() -> String {
    use windows::{
        core::factory,
        Graphics::Capture::GraphicsCaptureItem,
        Win32::{
            Foundation::HWND as Handle, Graphics::Gdi::HMONITOR,
            System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop,
        },
    };
    use windows_sys::Win32::{
        Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTOPRIMARY},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, WS_EX_TOOLWINDOW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
        },
    };
    let Ok(interop) = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>() else {
        return "probe: no interop factory".to_owned();
    };
    let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
    let mut results = Vec::new();
    for (name, extended) in [("plain window", 0), ("tool window", WS_EX_TOOLWINDOW)] {
        // SAFETY: a private window of the system STATIC class, destroyed right after the attempt.
        unsafe {
            let hwnd = CreateWindowExW(
                extended,
                class.as_ptr(),
                std::ptr::null(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                500,
                300,
                200,
                150,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            let result = interop.CreateForWindow::<GraphicsCaptureItem>(Handle(hwnd.cast()));
            results.push(format!("{name}: {:?}", result.map(|_| "ok")));
            DestroyWindow(hwnd);
        }
    }
    // SAFETY: a plain monitor query.
    let monitor = unsafe {
        MonitorFromPoint(
            windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
            MONITOR_DEFAULTTOPRIMARY,
        )
    };
    // SAFETY: the handle comes from the monitor query above.
    let result =
        unsafe { interop.CreateForMonitor::<GraphicsCaptureItem>(HMONITOR(monitor.cast())) };
    results.push(format!("primary monitor: {:?}", result.map(|_| "ok")));
    format!("probe: {}", results.join("; "))
}
