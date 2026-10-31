//! トレイの状態とイベントの表（`tray::tests` の先頭のコメント）のうち、書き込み中の行。

use super::*;
use crate::test_support::stub_window;

/// 撮影を始めるトレイのイベント（`tray::tests::STARTS` と同じ）。どれも `begin_capture` に届く。
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

/// 範囲の中に横線を 1 本引く。
fn stroke(desk: &Desk) {
    desk.drag(s().overlay, (220, 240), &[(260, 240)], (300, 240));
    assert_eq!(marks(), 1);
}

#[test]
#[ignore = "Covers the screen"]
fn tray_requests_leave_a_marked_up_snip_as_it_was() {
    let desk = Desk::start_with(|settings| {
        settings.copy = false;
        settings.history = true;
        settings.notify = false;
    });
    mark_up(&desk);
    stroke(&desk);
    let stop = ("the stop command", WM_COMMAND, TRAY_STOP, 0);
    for (name, message, wp, lp) in STARTS.into_iter().chain([stop]) {
        desk.send(desk.main, message, wp, lp);
        let session = s().markup_session.as_ref().expect(name);
        assert_eq!(session.marks.len(), 1, "{name} keeps the stroke");
        let a = session.area;
        assert_eq!(
            (a.left, a.top, a.right, a.bottom),
            (200, 200, 360, 280),
            "{name}"
        );
        assert!(
            s().active && shown(s().overlay) && shown(s().markup_bar),
            "{name}"
        );
        assert!(!shown(s().toolbar), "{name} brings no toolbar back");
        assert!(crate::history::list().is_empty(), "{name} captured nothing");
    }
    // 書き込みはそのまま続き、Enter で確定できる。
    desk.key(s().overlay, VK_RETURN);
    assert!(!s().active);
    assert_eq!(crate::history::list().len(), 1);
}

#[test]
#[ignore = "Covers the screen"]
fn our_windows_stay_hidden_while_marking_up_and_return_after_it() {
    for (name, key) in [("Enter", VK_RETURN), ("Escape", VK_ESCAPE)] {
        let desk = Desk::start_with(|settings| {
            settings.copy = false;
            settings.notify = false;
        });
        let settings = stub_window(SETTINGS_WINDOW);
        // SAFETY: the window belongs to this test.
        unsafe { ShowWindow(settings, SW_SHOWNOACTIVATE) };
        mark_up(&desk);
        stroke(&desk);
        assert!(!shown(settings), "hidden while marking up");
        assert_eq!(s().hidden, vec![SETTINGS_WINDOW]);
        desk.key(s().overlay, key);
        assert!(s().markup_session.is_none(), "{name}");
        assert!(shown(settings), "{name} brings it back");
        assert!(s().hidden.is_empty(), "{name}");
        unsafe { DestroyWindow(settings) };
        drop(desk);
    }
}

#[test]
#[ignore = "Covers the screen"]
fn switching_the_kept_toolbar_off_while_marking_up_keeps_it_closed_afterwards() {
    for switched_off in [false, true] {
        let desk = Desk::start_with(|settings| {
            settings.keep_toolbar = true;
            settings.copy = false;
            settings.history = true;
            settings.notify = false;
        });
        mark_up(&desk);
        assert!(
            s().toolbar_open && !shown(s().toolbar),
            "the kept toolbar waits out of sight"
        );
        if switched_off {
            let mut settings = config::load_settings();
            settings.keep_toolbar = false;
            config::save_settings(&settings).unwrap();
            desk.send(desk.main, TRAY_RELOAD, 0, 0);
            assert!(s().markup_session.is_some(), "marking up goes on");
        }
        desk.key(s().overlay, VK_RETURN);
        assert_eq!(crate::history::list().len(), 1);
        assert_eq!(
            s().toolbar_open,
            !switched_off,
            "switched_off={switched_off}"
        );
        assert_eq!(
            shown(s().toolbar),
            !switched_off,
            "switched_off={switched_off}"
        );
        drop(desk);
    }
}
