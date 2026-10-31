//! 設定の窓がトレイへ送る知らせ（設定の読み直し、履歴を開く）の宛先。

use super::*;
use crate::test_support::others::Others;

#[test]
#[ignore = "Creates windows in another process"]
fn in_tests_the_settings_tell_only_a_tray_of_their_own_process() {
    let _captions = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
    let sandbox = Sandbox::new();
    let others = Others::open(&sandbox);
    // SAFETY: a plain window query.
    let other_tray = unsafe { ui::find_class_window(MAIN, Some(others.id())) };
    assert!(!other_tray.is_null(), "the other process has a tray window");
    // テストの設定の窓は、ほかのプロセスのトレイ（ほかのテストや、利用者が動かしている
    // Snipping Lite）に知らせを送らない。
    assert!(tray_window().is_null());
}
