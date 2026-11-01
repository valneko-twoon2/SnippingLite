use super::*;

#[test]
#[ignore = "Requires an unlocked Windows desktop and Windows Graphics Capture"]
fn closing_the_window_tells_the_capture_that_it_ended() {
    let fixture = CoveredWindow::new();
    let mut capture =
        WindowCapture::new(Target::new(fixture.target, false), 320, 240, false).unwrap();
    capture.grab().unwrap();
    assert!(!capture.closed.load(Ordering::Acquire));
    // 窓を閉じると、撮影の対象（GraphicsCaptureItem）が閉じたと知らせてくる。録画はこれで終わる。
    // 知らせは窓のスレッドのメッセージを流している間に届くので、待つ間も流す。
    // SAFETY: the window belongs to this test.
    unsafe { DestroyWindow(fixture.target) };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !capture.closed.load(Ordering::Acquire) {
        assert!(
            Instant::now() < deadline,
            "the capture was not told that its window closed"
        );
        // SAFETY: plain message loop calls on this thread's queue.
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            for _ in 0..crate::test_support::desk::MESSAGE_LIMIT {
                if PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) == 0 {
                    break;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(capture.grab().is_err());
}
