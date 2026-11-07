//! docs/behavior.md: 範囲のモードは、アプリを終えるまで最後に選んだものが続く。ウィンドウのモードのまま始めた
//! 切り取りは、マウスを動かす前から、ポインターの下の窓を強調する（クリックすればその窓を撮る）。

use super::*;

/// ポインターを中心にした 200 x 160 の、このテストの最前面の窓。机の上の様子によらず、ポインターの
/// 下にはこの窓がある。
unsafe fn window_under_the_pointer() -> (HWND, POINT) {
    let mut cursor = POINT { x: 0, y: 0 };
    GetCursorPos(&mut cursor);
    let hwnd = CreateWindowExW(
        // 撮る窓に WS_EX_TOOLWINDOW や WS_EX_NOACTIVATE は付けない。窓の撮影が拒む。
        WS_EX_TOPMOST,
        w("STATIC").as_ptr(),
        w("Snipping Lite window mode test").as_ptr(),
        WS_POPUP,
        cursor.x - 100,
        cursor.y - 80,
        200,
        160,
        null_mut(),
        null_mut(),
        null_mut(),
        null(),
    );
    assert!(!hwnd.is_null());
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    DwmFlush();
    (hwnd, cursor)
}

#[test]
#[ignore = "Covers the screen"]
fn a_snip_in_the_kept_window_mode_highlights_the_window_under_the_pointer_at_once() {
    let desk = Desk::start();
    begin(&desk);
    // SAFETY: the windows belong to this test.
    unsafe { set_mode(MODE_WINDOW) };
    desk.key(s().overlay, VK_ESCAPE);
    assert!(!s().active);
    // SAFETY: a window of this test, destroyed below.
    let (window, cursor) = unsafe { window_under_the_pointer() };
    // マウスを動かさずに、次の切り取りを始める。
    unsafe { begin_capture() };
    assert!(s().active);
    assert_eq!(s().mode, MODE_WINDOW, "the mode is kept");
    assert_eq!(s().hover_hwnd, window, "the window under the pointer");
    // 強調の枠は、その窓を撮る範囲を、オーバーレイの座標で画面の中に切り詰めたもの。
    let origin = s().origin;
    let area = window_capture::Target::new(window, false).bounds().unwrap();
    let expected = (
        (area.left - origin.x).max(0),
        (area.top - origin.y).max(0),
        (area.right - origin.x).min(s().width),
        (area.bottom - origin.y).min(s().height),
    );
    let shown = s().hover_window.expect("a window is highlighted");
    assert_eq!((shown.left, shown.top, shown.right, shown.bottom), expected);
    let pointer = (cursor.x - origin.x, cursor.y - origin.y);
    assert!(
        (shown.left..shown.right).contains(&pointer.0)
            && (shown.top..shown.bottom).contains(&pointer.1),
        "{pointer:?} is inside {expected:?}"
    );
    desk.key(s().overlay, VK_ESCAPE);
    // SAFETY: the window belongs to this test.
    unsafe { DestroyWindow(window) };
}
