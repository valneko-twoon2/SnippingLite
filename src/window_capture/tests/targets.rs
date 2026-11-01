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

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT {
        left,
        top,
        right,
        bottom,
    }
}

fn size(width: i32, height: i32) -> SizeInt32 {
    SizeInt32 {
        Width: width,
        Height: height,
    }
}

#[test]
fn the_part_of_the_picture_to_copy_follows_the_crop_in_proportion_to_the_frame() {
    // 320x240 の枠から縁の 1 px を落とした範囲を、同じ大きさの絵から写す。左上は 1 * 320 / 320 = 1、
    // 幅は絵の残り 319 と出力の 318 の小さいほう。
    let full = rect(100, 50, 420, 290);
    assert_eq!(
        source_box(full, rect(101, 51, 419, 289), size(320, 240), (318, 238)),
        Some((1, 1, 318, 238))
    );
    // 絵が枠の 2 倍（拡大の違う画面）なら、位置も 2 倍: 10 * 400 / 200 = 20、31 * 200 / 100 = 62。
    // 高さは出力の 50 で切れ、幅は絵の残り 400 - 20 = 380。
    let full = rect(0, 0, 200, 100);
    assert_eq!(
        source_box(full, rect(10, 31, 190, 99), size(400, 200), (1000, 50)),
        Some((20, 62, 380, 50))
    );
    // 境界値: 1 px の枠でも割れる。
    let one = rect(7, 8, 8, 9);
    assert_eq!(source_box(one, one, size(1, 1), (1, 1)), Some((0, 0, 1, 1)));
    // 枠の外の範囲は絵の端に収め、絵の右端より先なら写すものがない。
    assert_eq!(
        source_box(full, rect(-50, -50, 100, 100), size(200, 100), (200, 100)),
        Some((0, 0, 200, 100))
    );
    assert_eq!(
        source_box(full, rect(200, 0, 260, 100), size(200, 100), (200, 100)),
        None
    );
    // 画面の端の大きな座標でも、引き算と掛け算があふれない: 2^31 * 100 / (2^32 - 1) = 50.00…
    let huge = rect(i32::MIN, i32::MIN, i32::MAX, i32::MAX);
    assert_eq!(
        source_box(huge, rect(0, 0, 1, 1), size(100, 100), (100, 100)),
        Some((50, 50, 50, 50))
    );
}

#[test]
fn a_frame_or_picture_without_width_or_height_copies_nothing_instead_of_dividing_by_zero() {
    // 枠を読んだ時に窓の幅か高さが 0 でも（大きさが変わる途中など）、0 で割って録画のスレッドを
    // 落とさずに、写すものがないと答える。幅か高さが負の枠も同じ。
    let crop = rect(100, 50, 420, 290);
    for full in [
        rect(100, 50, 100, 290),
        rect(100, 50, 420, 50),
        rect(100, 50, 100, 50),
        rect(420, 50, 100, 290),
        rect(100, 290, 420, 50),
    ] {
        assert_eq!(
            source_box(full, crop, size(320, 240), (320, 240)),
            None,
            "{:?}",
            bounds_tuple(full)
        );
    }
    // 絵や出力の大きさが 0 や負でも panic しない。
    let full = rect(100, 50, 420, 290);
    for (picture, limit) in [
        (size(0, 240), (320, 240)),
        (size(320, 0), (320, 240)),
        (size(-1, 240), (320, 240)),
        (size(320, 240), (0, 240)),
        (size(320, 240), (320, -1)),
    ] {
        assert_eq!(
            source_box(full, crop, picture, limit),
            None,
            "{picture:?} into {limit:?}"
        );
    }
}
