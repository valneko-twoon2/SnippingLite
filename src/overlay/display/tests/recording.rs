//! カウントダウンと録画中に構成が変わったとき: 範囲の下のモニターが変わったときだけ取り消す・
//! 止めて保存する。ウィンドウのカウントダウンと録画は、窓を追いかけるので続ける。

use super::*;
use crate::window_capture::{test_support::CoveredWindow, Target};

#[test]
#[ignore = "Covers the screen"]
fn a_countdown_goes_on_unless_the_monitor_under_its_area_changed() {
    let desk = Desk::start_with(|settings| {
        settings.countdown = true;
        settings.system_audio = false;
    });
    select_recording(&desk, AREA);
    assert!(!s().count_window.is_null(), "counting down");
    // 範囲にかからないモニターを外した: 続ける。
    remember(with_one_more_on_the_left());
    display_change(&desk);
    assert!(
        !s().count_window.is_null(),
        "the area still shows the same screen"
    );
    assert_eq!(s().count_left, 3);
    // 範囲の下のモニターの解像度が変わった: 取り消し、何も録画しない。
    remember(narrower_at(area_corner()));
    display_change(&desk);
    assert!(s().count_window.is_null() && s().recording.is_none());
    assert!(s().count_points.is_empty() && s().count_target.is_none());
    assert!(!s().active);
}

#[test]
#[ignore = "Records the screen"]
fn a_recording_stops_and_is_saved_only_when_the_monitor_under_it_changed() {
    let desk = Desk::start_with(|settings| {
        settings.countdown = false;
        settings.system_audio = false;
        settings.notify = false;
    });
    select_recording(&desk, AREA);
    assert!(s().recording.is_some() && !s().record_window.is_null());
    assert!(desk.pump_until(Duration::from_secs(20), || {
        s().recording
            .as_ref()
            .is_some_and(|r| r.elapsed() > Duration::ZERO)
    }));
    // 表示を画面の外へやる。範囲にかからないモニターの変化では録画は続き、表示は範囲の真ん中に
    // 近いモニターの上部中央（幅 168 DIP、上から 12 DIP）に戻る。
    // SAFETY: the window belongs to this test.
    unsafe {
        SetWindowPos(
            s().record_window,
            null_mut(),
            -30000,
            -30000,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
    remember(with_one_more_on_the_left());
    display_change(&desk);
    assert!(
        s().recording.is_some(),
        "the area still shows the same screen"
    );
    let origin = s().origin;
    let center = RECT {
        left: origin.x + 300,
        top: origin.y + 260,
        right: origin.x + 301,
        bottom: origin.y + 261,
    };
    let (monitor, _) = monitor_of(center);
    let scale = s().scale;
    let width = (168.0 * scale).round() as i32;
    let left = (monitor.left + monitor.right - width) / 2;
    let top = monitor.top + (12.0 * scale).round() as i32;
    assert_eq!(
        edges(bounds(s().record_window)),
        (left, top, left + width, top + (48.0 * scale).round() as i32),
        "the indicator is back at the top center"
    );
    // 範囲の下のモニターが変わった: 止めて、そこまでの動画を残す。
    remember(narrower_at(area_corner()));
    display_change(&desk);
    assert!(s().recording.is_none() && s().record_window.is_null());
    let videos = std::fs::read_dir(desk.sandbox.path().join("videos"))
        .unwrap()
        .count();
    assert_eq!(videos, 1, "the video up to the change is kept");
}
/// 録画が始まり、最初のフレームを書くまで待つ。
fn wait_started(desk: &Desk) {
    assert!(desk.pump_until(Duration::from_secs(20), || {
        s().recording
            .as_ref()
            .is_some_and(|r| r.elapsed() > Duration::ZERO || r.failed())
    }));
}

#[test]
#[ignore = "Records the screen"]
fn a_window_countdown_and_recording_follow_the_window_through_a_new_layout() {
    let desk = Desk::start_with(|settings| {
        settings.countdown = true;
        settings.system_audio = false;
        settings.notify = false;
    });
    let fixture = CoveredWindow::new();
    s().video = true;
    start(&desk, true);
    // ポインターの下の窓は机の上の様子で変わるので、的の窓を選んだことにする。
    // SAFETY: the windows belong to this test.
    unsafe { set_mode(MODE_WINDOW) };
    let window = Target::new(fixture.target, false).bounds().unwrap();
    let origin = s().origin;
    s().hover_hwnd = fixture.target;
    s().hover_window = Some(RECT {
        left: window.left - origin.x,
        top: window.top - origin.y,
        right: window.right - origin.x,
        bottom: window.bottom - origin.y,
    });
    desk.mouse(
        s().overlay,
        WM_LBUTTONDOWN,
        window.left - origin.x + 10,
        window.top - origin.y + 10,
    );
    let count = s().count_window;
    assert!(
        !count.is_null() && s().count_target.is_some(),
        "a window countdown"
    );
    // SAFETY: the window belongs to this test.
    unsafe { KillTimer(count, COUNT_TIMER) };
    // 窓の下のモニターの解像度が変わっても、窓を追いかけるので続ける。
    let under = POINT {
        x: window.left + 10,
        y: window.top + 10,
    };
    remember(narrower_at(under));
    display_change(&desk);
    assert!(!s().count_window.is_null(), "the window countdown goes on");
    for _ in 0..3 {
        desk.send(count, WM_TIMER, COUNT_TIMER, 0);
    }
    assert!(s().recording.is_some(), "the window recording started");
    wait_started(&desk);
    remember(narrower_at(under));
    display_change(&desk);
    assert!(s().recording.is_some(), "the window recording goes on");
    // SAFETY: the state belongs to this test.
    unsafe { stop_recording() };
    let videos = std::fs::read_dir(desk.sandbox.path().join("videos"))
        .unwrap()
        .count();
    assert_eq!(videos, 1);
    drop(fixture);
}
