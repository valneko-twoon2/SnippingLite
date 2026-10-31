//! トレイの状態とイベントの表。状態ごとに、`main_proc` に届く各イベントの期待される振る舞いと、
//! それを確かめるテスト（なければ書かない理由）を並べる。期待値は README とコードの意図（コメント）
//! から決める。トレイのアイコンの左クリック、ショートカット、NEW はどれも `begin_capture` に届く
//! 同値のクラスなので、まとめて「開始」と書き、1 つのテストの中で 3 つとも送る。
//! 右クリックのメニューは録画中かどうかだけで変わる（`the_menu_offers_stop_recording_first_only_while_recording`）。
//! WM_CLOSE は状態によらずトレイのウィンドウを隠すだけ（既存 `the_delay_timer_takes_the_picture_and_closing_only_hides`）。
//!
//! 待機（トレイだけ。残したツールバーが出ていることもある）
//! - 開始 → 切り取りを始める（既存 `the_tray_commands_start_a_snip_and_open_the_windows`。
//!   残したツールバーはそのまま使う: 既存 `a_kept_toolbar_hides_for_the_picture_and_returns_after_it`）
//! - 右クリック → 「Snipping Lite」「About」「終了」
//! - TRAY_SETTINGS、TRAY_HISTORY → そのウィンドウを開く（既存 `the_tray_commands_start_a_snip_and_open_the_windows`）
//! - TRAY_STOP → 何もしない（既存、同上）
//! - TRAY_EXIT → アイコンを消して終わる（既存 `exiting_removes_the_tray_icon_and_ends_the_message_loop`）
//! - TRAY_RELOAD → 設定を反映し、keep_toolbar をオフにすると残したツールバーを閉じる
//!   （既存 `new_settings_reach_the_running_tray_process`）
//!
//! 切り取り中（オーバーレイとツールバー）
//! - 開始 → 何もしない。選んでいる途中の範囲も続く（`starting_again_while_snipping_leaves_the_selection_going`）
//! - TRAY_RELOAD → 使っているツールバーは隠さず、終えた後は出さない（既存 `new_settings_reach_the_running_tray_process`
//!   が `active` を手で立てて確かめる。終えた後は書き込み中と同じ経路で、下のテストが確かめる）
//! - TRAY_STOP → 書かない: メニューに出ないので届かない
//! - TRAY_SETTINGS → 書かない: 撮影を続けたまま設定を出すか、取り消すかを仕様が決めていない（質問）
//! - TRAY_HISTORY → 書かない: 送り手の設定ウィンドウは撮影の間は隠れている
//! - TRAY_EXIT → 書かない: プロセスごと終わる
//!
//! 書き込み中（切り取った範囲と書き込みのバー。ツールバーは隠れる）
//! - 開始、TRAY_STOP → 何もしない。書き込みもバーも残る（`markup::tests::tray::tray_requests_leave_a_marked_up_snip_as_it_was`）
//! - TRAY_RELOAD（keep_toolbar をオフ）→ 確定の後に残したツールバーを出さない。オフにしなければ出す
//!   （`markup::tests::tray::switching_the_kept_toolbar_off_while_marking_up_keeps_it_closed_afterwards`）
//! - 隠した自分のウィンドウ → 書き込みの間は隠れたまま、終えた後に戻る
//!   （`markup::tests::tray::our_windows_stay_hidden_while_marking_up_and_return_after_it`）
//! - Enter、Ctrl+C、キャプチャのボタン → 書き込みを重ねて確定する。Escape、× → 捨てる。
//!   Ctrl+Z、Ctrl+Y → 元に戻す、やり直す（`markup::tests::keys`、既存の `markup::tests`）
//! - TRAY_SETTINGS → 書かない: 切り取り中と同じ質問
//! - TRAY_EXIT → 書かない: 書き込み中の切り取りを捨てて終わるか、確定するかを仕様が決めていない（質問）
//!
//! 遅延の待ち（`pending`）
//! - 遅延の設定を外したので、利用者の操作では到達しない。開始で何もしないことと、タイマーで撮ることは
//!   既存 `the_delay_waits_before_taking_the_picture` と `the_delay_timer_takes_the_picture_and_closing_only_hides`
//!   が状態を手で作って確かめる。ほかのセルは書かない
//!
//! カウントダウン
//! - 開始 → 取り消し、撮影も録画も始めない（`recording::starting_events_cancel_a_countdown_and_start_nothing`）
//! - TRAY_RELOAD（keep_toolbar をオフ）→ 取り消しの後に残したツールバーを出さない。オフにしなければ出す
//!   （`recording::a_kept_toolbar_returns_after_a_cancelled_countdown_unless_switched_off_meanwhile`）
//! - 右クリック → 待機と同じ（録画はまだ始まっていない）
//! - TRAY_STOP → 書かない: メニューに出ないので届かない
//! - TRAY_SETTINGS、TRAY_HISTORY → 書かない: 録画中と同じ質問
//! - TRAY_EXIT → 書かない: カウントダウンのウィンドウはプロセスとともに消える
//!
//! 録画中
//! - 開始 → 録画を止めてファイルを仕上げ、新しい切り取りは始めない（`recording::starting_events_stop_a_recording_and_start_nothing`）
//! - 右クリック → 先頭に「録画を停止」が加わる
//! - TRAY_STOP → 止める（既存 `a_failed_recorder_stops_itself_at_the_next_tick`）
//! - TRAY_EXIT → ファイルを仕上げて終わる（既存 `a_recording_is_finished_when_the_program_exits`）
//! - TRAY_RELOAD（keep_toolbar をオフ）→ 停止の後に残したツールバーを出さない。オフにしなければ出す
//!   （`recording::a_kept_toolbar_returns_after_a_recording_unless_switched_off_meanwhile`）
//! - TRAY_SETTINGS、TRAY_HISTORY → 書かない: 開いたウィンドウが録画に映ってよいか、仕様にない（質問）

use super::*;
use crate::test_support::desk::Desk;

mod recording;

fn s() -> &'static mut State {
    // SAFETY: the test owns the state and runs on the thread that created it.
    unsafe { state() }
}

fn shown(window: HWND) -> bool {
    // SAFETY: plain window query.
    unsafe { IsWindowVisible(window) != 0 }
}

/// 撮影を始めるトレイのイベント。どれも `begin_capture` に届く。
const STARTS: [(&str, u32, WPARAM, LPARAM); 3] = [
    (
        "the icon's left click",
        TRAY_MESSAGE,
        1,
        WM_LBUTTONUP as LPARAM,
    ),
    ("the shortcut", WM_HOTKEY, HOTKEY as WPARAM, 0),
    ("the NEW command", WM_COMMAND, NEW, 0),
];

/// 保存された設定で「撮影後もツールバーを表示したままにする」をオフにし、トレイに読み直させる。
fn switch_the_kept_toolbar_off(desk: &Desk) {
    let mut settings = config::load_settings();
    settings.keep_toolbar = false;
    config::save_settings(&settings).unwrap();
    desk.send(desk.main, TRAY_RELOAD, 0, 0);
}

/// メニューの項目（コマンド、文字）を上から順に。
unsafe fn entries(menu: HMENU) -> Vec<(usize, String)> {
    (0..GetMenuItemCount(menu))
        .map(|i| {
            let mut text = [0u16; 64];
            let length = GetMenuStringW(menu, i as u32, text.as_mut_ptr(), 64, MF_BYPOSITION);
            let text = String::from_utf16_lossy(&text[..length.max(0) as usize]);
            (GetMenuItemID(menu, i) as usize, text)
        })
        .collect()
}

#[test]
fn the_menu_offers_stop_recording_first_only_while_recording() {
    for recording in [false, true] {
        // SAFETY: the menu is made, read and destroyed here; it is never shown.
        unsafe {
            let menu = tray_menu(recording);
            let mut expected = vec![
                // 最初の項目はアイコンの名前で、アプリのウィンドウ（設定）を開く。
                (TRAY_SETTINGS, "Snipping Lite".to_owned()),
                (TRAY_ABOUT, tr("About").to_owned()),
                (TRAY_EXIT, tr("Exit").to_owned()),
            ];
            if recording {
                // README: トレイのメニューの「録画を停止」で録画を終える。
                expected.insert(0, (TRAY_STOP, "録画を停止".to_owned()));
            }
            assert_eq!(entries(menu), expected, "recording={recording}");
            // 既定の項目（太字で、ダブルクリックで選ばれる）は、録画中でもアプリのウィンドウ。
            assert_eq!(GetMenuDefaultItem(menu, 0, 0), TRAY_SETTINGS as u32);
            DestroyMenu(menu);
        }
    }
}

#[test]
#[ignore = "Covers the screen"]
fn starting_again_while_snipping_leaves_the_selection_going() {
    let desk = Desk::start_with(|settings| {
        settings.copy = false;
        settings.history = true;
        settings.notify = false;
    });
    // SAFETY: the windows belong to this test.
    unsafe { begin_capture() };
    assert!(s().active);
    // 範囲を選んでいる途中（ボタンを押したまま動かしている）。
    desk.mouse(s().overlay, WM_MOUSEMOVE, 200, 200);
    desk.mouse(s().overlay, WM_LBUTTONDOWN, 200, 200);
    desk.mouse(s().overlay, WM_MOUSEMOVE, 260, 240);
    let points = s().points.len();
    for (name, message, wp, lp) in STARTS {
        desk.send(desk.main, message, wp, lp);
        assert!(s().active && s().dragging, "{name}");
        assert_eq!(
            (s().start.x, s().start.y, s().end.x, s().end.y),
            (200, 200, 260, 240),
            "{name}"
        );
        assert_eq!(s().points.len(), points, "{name} took no new picture");
        assert_eq!(s().mode, MODE_RECT, "{name}");
        assert!(shown(s().overlay) && shown(s().toolbar), "{name}");
        assert!(crate::history::list().is_empty(), "{name}");
    }
    // 選び終えると、最初に押した所からの範囲が撮れる。
    desk.mouse(s().overlay, WM_LBUTTONUP, 300, 260);
    assert!(!s().active);
    let entry = crate::history::list().remove(0);
    let (width, height, _) = imaging::decode(&entry.image).unwrap();
    assert_eq!((width, height), (100, 60));
}
