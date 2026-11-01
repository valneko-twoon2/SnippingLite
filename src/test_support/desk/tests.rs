//! `Desk` の後片付け。窓を表示したまま途中で失敗したテストも、止まらずに失敗として終わる。
//! 失敗したテストが止まると、ランナーは失敗を報告できず、CPU を使い続ける。

use super::*;
use crate::test_support::{
    child::{self, Child},
    crash::test_name,
    stub_window,
};

/// 子のテストが終わるのを待つ時間。テストのバイナリーの起動を含む。
const LIMIT: Duration = Duration::from_secs(60);
const FAILURE: &str = "the test fails with a window left on the screen";

#[test]
#[ignore = "Runs only as the child process of a desk test"]
fn child_fails_with_a_window_left_shown() {
    if child::value().is_none() {
        return;
    }
    let _desk = Desk::start();
    let window = stub_window(crate::SETTINGS_WINDOW);
    // 表示したまま、メッセージを流さずに失敗する。窓には描画の依頼（WM_PAINT）が残る。
    // SAFETY: the window belongs to this test.
    unsafe { ShowWindow(window, SW_SHOWNOACTIVATE) };
    panic!("{FAILURE}");
}

#[test]
#[ignore = "Creates the tray window"]
fn a_test_that_fails_with_a_window_shown_ends_and_reports_the_failure() {
    let sandbox = Sandbox::new();
    let name = test_name(module_path!(), "child_fails_with_a_window_left_shown");
    let mut child = Child::start(&sandbox, &name, "1");
    let status = child.wait(LIMIT);
    let said = child.said();
    let status =
        status.unwrap_or_else(|| panic!("the child test did not end within {LIMIT:?}:\n{said}"));
    assert!(!status.success(), "the child test fails:\n{said}");
    assert!(
        said.contains(FAILURE) && said.contains("1 failed"),
        "the failure is reported:\n{said}"
    );
    // 上限に頼らずに終わる: 残った窓を隠すので、捨てるメッセージが尽きる。
    assert!(
        !said.contains("more kept coming"),
        "the window left shown is hidden, so the messages run out:\n{said}"
    );
}

/// トレイを終了させたテスト（トレイの窓がもう無い）の片付けでも、隠すのはテストのスレッドの窓
/// だけで、ほかのスレッド（利用者のアプリやタスクバーに当たる）の窓には触れない。
#[test]
#[ignore = "Creates the tray window"]
fn putting_away_a_desk_without_its_tray_window_leaves_other_threads_alone() {
    let (sent, received) = std::sync::mpsc::channel();
    let (done, finish) = std::sync::mpsc::channel::<()>();
    let other = std::thread::spawn(move || {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        // SAFETY: a plain window of this thread, off the screen.
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class.as_ptr(),
                std::ptr::null(),
                WS_POPUP | WS_VISIBLE,
                -32000,
                -32000,
                10,
                10,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        sent.send(window as isize).unwrap();
        let _ = finish.recv();
        // SAFETY: the window belongs to this thread.
        unsafe { DestroyWindow(window) };
    });
    let window = received.recv().unwrap() as HWND;
    assert!(!window.is_null());
    let desk = Desk::start();
    // トレイの終了と同じく、トレイの窓が先に無くなる。
    // SAFETY: the window belongs to this test.
    unsafe { DestroyWindow(desk.main) };
    drop(desk);
    // SAFETY: a plain window query.
    let shown = unsafe { IsWindowVisible(window) } != 0;
    done.send(()).unwrap();
    other.join().unwrap();
    assert!(shown, "the window of another thread stays shown");
}
