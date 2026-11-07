//! 表のカウントダウンと録画中の行。

use super::*;
use std::time::Duration;

/// 録画の保存先にできたファイル。
fn videos(desk: &Desk) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(desk.sandbox.path().join("videos"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|f| f.path())
        .collect();
    files.sort();
    files
}

/// 録画の切り取りを始め、範囲を選ぶ。カウントダウンがオンならカウントダウンが、オフなら録画が始まる。
fn select_video(desk: &Desk) {
    s().video = true;
    // SAFETY: the windows belong to this test.
    unsafe { begin_capture() };
    assert!(s().active);
    desk.drag(s().overlay, (200, 200), &[], (400, 320));
    assert!(!s().active, "the overlay closed");
}

/// 録画を始め、最初のフレームが書かれるまで待つ。
fn record(desk: &Desk) {
    select_video(desk);
    assert!(s().recording.is_some(), "recording");
    assert!(desk.pump_until(Duration::from_secs(20), || {
        s().recording
            .as_ref()
            .is_some_and(|r| r.elapsed() > Duration::ZERO || r.failed())
    }));
    desk.pump(Duration::from_millis(300));
}

#[test]
#[ignore = "Covers the screen"]
fn starting_events_cancel_a_countdown_and_start_nothing() {
    let desk = Desk::start_with(|settings| {
        settings.countdown = true;
        settings.system_audio = false;
        settings.notify = false;
    });
    for (name, message, wp, lp) in STARTS {
        select_video(&desk);
        let countdown = s().count_window;
        assert!(!countdown.is_null(), "the countdown runs");
        desk.send(desk.main, message, wp, lp);
        assert!(s().count_window.is_null(), "{name} cancels the countdown");
        // SAFETY: plain window query.
        assert_eq!(
            unsafe { IsWindow(countdown) },
            0,
            "{name}: its timer is gone"
        );
        assert!(
            s().recording.is_none() && !s().active,
            "{name} starts nothing"
        );
        assert!(!shown(s().overlay), "{name}");
    }
    assert!(videos(&desk).is_empty());
}

#[test]
#[ignore = "Covers the screen"]
fn a_kept_toolbar_returns_after_a_cancelled_countdown_unless_switched_off_meanwhile() {
    for switched_off in [false, true] {
        let desk = Desk::start_with(|settings| {
            settings.keep_toolbar = true;
            settings.countdown = true;
            settings.system_audio = false;
            settings.notify = false;
        });
        select_video(&desk);
        assert!(!s().count_window.is_null());
        assert!(
            s().toolbar_open && !shown(s().toolbar),
            "the kept toolbar waits out of sight"
        );
        if switched_off {
            switch_the_kept_toolbar_off(&desk);
        }
        // カウントダウンをクリックして取り消す。
        desk.mouse(s().count_window, WM_LBUTTONUP, 10, 10);
        assert!(s().count_window.is_null() && s().recording.is_none());
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

#[test]
#[ignore = "Covers the screen"]
fn settings_or_history_asked_for_during_the_countdown_open_nothing() {
    let desk = Desk::start_with(|settings| {
        settings.countdown = true;
        settings.system_audio = false;
        settings.notify = false;
    });
    let windows = closed_windows();
    select_video(&desk);
    let countdown = s().count_window;
    assert!(!countdown.is_null(), "the countdown runs");
    // カウントダウンはキーボードを受け取らないので、キーボードは確かめない。
    assert_nothing_opens(&desk, &windows, null_mut(), || {
        s().count_window == countdown && shown(countdown) && s().recording.is_none()
    });
    // これまでどおり、クリックで取り消せる。
    desk.mouse(countdown, WM_LBUTTONUP, 10, 10);
    assert!(s().count_window.is_null() && s().recording.is_none());
}

#[test]
#[ignore = "Records the screen"]
fn settings_and_history_open_while_recording() {
    // 録画中に開いた画面は、録画に写ってよい（docs/behavior.md）。
    let desk = Desk::start_with(|settings| {
        settings.countdown = false;
        settings.system_audio = false;
        settings.notify = false;
        settings.copy_video = false;
    });
    let windows = closed_windows();
    record(&desk);
    for ((name, command), window) in OPENS.into_iter().zip(windows.0) {
        desk.send(desk.main, WM_COMMAND, command, 0);
        assert!(shown(window), "{name} opens");
        assert!(s().recording.is_some(), "{name} leaves the recording going");
    }
    desk.send(desk.main, WM_COMMAND, TRAY_STOP, 0);
    assert!(s().recording.is_none());
    assert_eq!(videos(&desk).len(), 1, "the recording is finished");
}

#[test]
#[ignore = "Records the screen"]
fn starting_events_stop_a_recording_and_start_nothing() {
    let desk = Desk::start_with(|settings| {
        settings.countdown = false;
        settings.system_audio = false;
        settings.notify = false;
        settings.copy_video = false;
    });
    for (index, (name, message, wp, lp)) in STARTS.into_iter().enumerate() {
        record(&desk);
        desk.send(desk.main, message, wp, lp);
        assert!(s().recording.is_none(), "{name} stops the recording");
        assert!(
            s().record_window.is_null() && s().record_frame.is_null(),
            "{name}"
        );
        assert!(!s().active && !shown(s().overlay), "{name} starts no snip");
        let files = videos(&desk);
        assert_eq!(files.len(), index + 1, "{name}");
        for file in files {
            assert_eq!(&std::fs::read(&file).unwrap()[4..8], b"ftyp", "{name}");
        }
    }
}

#[test]
#[ignore = "Records the screen"]
fn a_kept_toolbar_returns_after_a_recording_unless_switched_off_meanwhile() {
    for switched_off in [false, true] {
        let desk = Desk::start_with(|settings| {
            settings.keep_toolbar = true;
            settings.countdown = false;
            settings.system_audio = false;
            settings.notify = false;
            settings.copy_video = false;
        });
        record(&desk);
        assert!(
            s().toolbar_open && !shown(s().toolbar),
            "the kept toolbar waits out of sight"
        );
        if switched_off {
            switch_the_kept_toolbar_off(&desk);
        }
        desk.send(desk.main, WM_COMMAND, TRAY_STOP, 0);
        assert!(s().recording.is_none());
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
