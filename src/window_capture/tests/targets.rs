use super::*;
use crate::test_support::{others::Others, Sandbox, CAPTIONS};
use std::ptr::{null, null_mut};
use windows_sys::Win32::UI::HiDpi::{
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

#[test]
fn a_borderless_window_loses_its_edge_unless_it_fills_its_screen() {
    // SAFETY: the windows belong to this test, and the DPI awareness is put back.
    unsafe {
        // 枠の座標を、拡大の影響なしに画面の画素で読む（`CoveredWindow` と同じ）。
        let dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // 重ならない窓は、半透明の縁の 1 px を上下左右で落とす。3 px 四方なら真ん中の 1 px が
        // 残り、2 px 四方なら何も残らない。何も残らなければ、ほかのものを撮りもしない。
        let three = popup(100, 100, 3, 3);
        assert_eq!(
            bounds_tuple(Target::new(three, false).bounds().unwrap()),
            (101, 101, 102, 102)
        );
        let two = popup(100, 100, 2, 2);
        assert!(Target::new(two, false).bounds().is_err());
        assert!(snapshot(Target::new(two, false)).is_err());
        // 画面いっぱいの窓（全画面のゲームや動画）は、縁まで画面なので、何も落とさない。
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let monitor = MonitorFromWindow(three, MONITOR_DEFAULTTONEAREST);
        assert_ne!(GetMonitorInfoW(monitor, &mut info), 0);
        let m = info.rcMonitor;
        let (width, height) = (m.right - m.left, m.bottom - m.top);
        let full = popup(m.left, m.top, width, height);
        assert_eq!(
            bounds_tuple(Target::new(full, false).bounds().unwrap()),
            bounds_tuple(m)
        );
        // 1 px ずれれば画面いっぱいではないので、縁を落とす。
        let flags = SWP_NOZORDER | SWP_NOACTIVATE;
        SetWindowPos(full, null_mut(), m.left + 1, m.top, width, height, flags);
        assert_eq!(
            bounds_tuple(Target::new(full, false).bounds().unwrap()),
            (m.left + 2, m.top + 1, m.right, m.bottom - 1)
        );
        for window in [three, two, full] {
            DestroyWindow(window);
        }
        if !dpi.is_null() {
            SetThreadDpiAwarenessContext(dpi);
        }
    }
}

#[test]
#[ignore = "Creates windows in another process"]
fn a_uwp_frame_belongs_to_the_process_of_the_app_inside_it() {
    let _captions = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = Sandbox::new();
    let others = Others::open(&sandbox);
    // SAFETY: plain window query.
    let app = unsafe { crate::ui::find_app_window(&others.identity, Some(others.id())) };
    assert!(!app.is_null(), "the window of the other process");
    // SAFETY: the class and the frame are made and destroyed here; the other process's
    // window is only moved into the frame, and goes away with its process.
    unsafe {
        let class = crate::w("ApplicationFrameWindow");
        let instance = GetModuleHandleW(null());
        let spec = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&spec);
        let frame = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            10,
            10,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        assert!(!frame.is_null());
        // 中に何もない枠の音は、枠のプロセスのもの。
        assert_eq!(
            Target::new(frame, false).process_id(),
            Some(std::process::id())
        );
        // UWP のアプリは、枠（ApplicationFrameWindow）の中の、自分のプロセスの窓に描く。音を
        // 出すのはそのプロセスなので、枠を選んでも、中の窓のプロセスの音を録る。
        assert!(
            !SetParent(app, frame).is_null(),
            "the app moves into the frame"
        );
        assert_eq!(Target::new(frame, false).process_id(), Some(others.id()));
        drop(others);
        DestroyWindow(frame);
        UnregisterClassW(class.as_ptr(), instance);
    }
}
