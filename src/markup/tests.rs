use super::bar::*;
use super::*;
use crate::test_support::{desk::Desk, Sandbox};
use std::time::Duration;

const SCALES: [f32; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];

fn s() -> &'static mut State {
    // SAFETY: the test owns the state and runs on the thread that created it.
    unsafe { state() }
}

fn shown(window: HWND) -> bool {
    // SAFETY: plain window query.
    unsafe { IsWindowVisible(window) != 0 }
}

fn line(points: &[(f32, f32)], radius: f32) -> Mark {
    Mark {
        highlight: false,
        color: annotate::COLORS[0],
        radius,
        points: points.to_vec(),
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

#[test]
fn the_eraser_reaches_a_line_up_to_its_width_plus_the_reach() {
    let mark = line(&[(0.0, 0.0), (100.0, 0.0)], 2.0);
    // 境界値: 線の太さ 2 と届く距離 8 の合計 10 までは触れる。
    assert!(touches(&mark, (50.0, 10.0), 8.0));
    assert!(!touches(&mark, (50.0, 10.5), 8.0));
    // 端の先は、端からの距離で決まる。
    assert!(touches(&mark, (110.0, 0.0), 8.0));
    assert!(!touches(&mark, (110.5, 0.0), 8.0));
    // 1 点だけの線（クリック）と、点のない線。
    assert!(touches(&line(&[(5.0, 5.0)], 1.0), (5.0, 12.0), 6.0));
    assert!(!touches(&line(&[], 1.0), (0.0, 0.0), 100.0));
    // 同じ点が続いても割り算で壊れない。
    assert!(touches(
        &line(&[(3.0, 3.0), (3.0, 3.0)], 1.0),
        (3.0, 4.0),
        0.0
    ));
}

#[test]
fn every_bar_button_is_found_at_its_center_and_none_overlap() {
    for scale in SCALES {
        for index in 0..8 {
            let [l, t, r, b] = button(index, scale);
            let center = POINT {
                x: ((l + r) / 2.0) as i32,
                y: ((t + b) / 2.0) as i32,
            };
            assert_eq!(button_at(center, scale), index, "scale {scale}");
        }
        for (a, b) in (0..8).zip(1..8) {
            assert!(button(a, scale)[2] <= button(b, scale)[0], "{a} {b}");
        }
    }
    // ボタンの外は何もない。
    assert_eq!(button_at(POINT { x: 4, y: 24 }, 1.0), -1);
    assert_eq!(button_at(POINT { x: 116, y: 24 }, 1.0), -1);
    assert_eq!(button_at(POINT { x: 60, y: 44 }, 1.0), -1);
}

#[test]
fn the_bar_goes_below_the_area_then_above_then_inside() {
    let monitor = rect(0, 0, 1920, 1040);
    // 下に入る: 範囲の中央の下。
    let below = place(rect(500, 100, 900, 300), monitor, 1.0);
    assert_eq!((below.x, below.y), (700 - 216, 312));
    // 下に入らなければ上。
    let above = place(rect(500, 900, 900, 1030), monitor, 1.0);
    assert_eq!(above.y, 900 - 12 - 48);
    // どちらにも入らなければ（画面いっぱい）範囲の中の下端。
    let inside = place(monitor, monitor, 1.0);
    assert_eq!(inside.y, 1040 - 24 - 48);
    // 画面の端では内側に寄せる。
    assert_eq!(place(rect(0, 100, 40, 200), monitor, 1.0).x, 12);
    assert_eq!(
        place(rect(1880, 100, 1920, 200), monitor, 1.0).x,
        1920 - 432 - 12
    );
    // 拡大率に合わせて大きさと間隔も広がる。
    let scaled = place(rect(500, 100, 900, 300), monitor, 2.0);
    assert_eq!((scaled.x, scaled.y), (700 - 432, 324));
}

#[test]
fn the_toggle_survives_a_restart_and_ignores_recordings() {
    let _sandbox = Sandbox::new();
    assert!(!config::load_markup(), "off until it is turned on");
    config::save_markup(true);
    assert!(config::load_markup());
    config::save_markup(false);
    assert!(!config::load_markup());
    drop(_sandbox);
    let desk = Desk::start();
    s().markup = true;
    s().video = false;
    // SAFETY: plain state reads.
    assert!(unsafe { wanted() });
    s().video = true;
    assert!(!unsafe { wanted() }, "recordings are not marked up");
    drop(desk);
}

/// ペンのボタンをオンにして、範囲を選んだところまで進める。
fn mark_up(desk: &Desk) {
    // SAFETY: the windows belong to this test.
    unsafe { begin_capture() };
    if !s().markup {
        let [l, t, r, b] = toolbar_button(EDIT_BUTTON, s().scale);
        desk.click(s().toolbar, ((l + r) / 2.0) as i32, ((t + b) / 2.0) as i32);
    }
    assert!(
        s().markup && config::load_markup(),
        "the toggle is on and saved"
    );
    assert!(s().active, "pressing it keeps the snip going");
    desk.drag(s().overlay, (200, 200), &[], (360, 280));
    desk.pump(Duration::from_millis(30));
}

fn marks() -> usize {
    s().markup_session.as_ref().map_or(0, |m| m.marks.len())
}

#[test]
#[ignore = "Covers the screen"]
fn a_marked_up_snip_is_drawn_on_undone_erased_and_captured() {
    // 画面を止めるときも、動かしたままのときも、同じ流れで書き込める。
    for freeze in [false, true] {
        let desk = Desk::start_with(|settings| {
            settings.freeze = freeze;
            settings.copy = false;
            settings.save = true;
            settings.history = true;
            settings.notify = false;
        });
        mark_up(&desk);
        assert!(
            s().active && s().markup_session.is_some(),
            "freeze={freeze}"
        );
        assert!(shown(s().overlay) && shown(s().markup_bar) && !shown(s().toolbar));
        // ペンで横線を引く。
        desk.drag(s().overlay, (220, 240), &[(260, 240)], (300, 240));
        assert_eq!(marks(), 1);
        // SAFETY: the windows belong to this test.
        unsafe {
            press(UNDO);
            assert_eq!(marks(), 0);
            press(REDO);
            assert_eq!(marks(), 1);
            // 消しゴムでなぞると線ごと消え、1 回で元に戻せる。
            press(ERASER);
        }
        desk.drag(s().overlay, (250, 230), &[(255, 238)], (260, 241));
        assert_eq!(marks(), 0);
        unsafe { press(UNDO) };
        assert_eq!(marks(), 1);
        // 範囲の外をクリックしても何も描かない。
        unsafe { press(PEN) };
        desk.click(s().overlay, 100, 100);
        assert_eq!(marks(), 1);
        unsafe { press(CAPTURE) };
        assert!(!s().active && s().markup_session.is_none());
        assert!(!shown(s().markup_bar) && !shown(s().overlay));
        let entry = crate::history::list().remove(0);
        let (width, height, pixels) = imaging::decode(&entry.image).unwrap();
        assert_eq!((width, height), (160, 80));
        // 線の中心（範囲の左上から 60, 40）は、ペンの色で塗られている。
        let at = ((40 * width + 60) * 4) as usize;
        let [r, g, b] = annotate::COLORS[config::load_pen().0];
        assert_eq!(pixels[at..at + 4], [b, g, r, 255], "freeze={freeze}");
        drop(desk);
    }
}

#[test]
#[ignore = "Covers the screen"]
fn escape_and_the_close_button_discard_a_marked_up_snip() {
    let desk = Desk::start_with(|settings| {
        settings.history = true;
        settings.notify = false;
    });
    mark_up(&desk);
    desk.key(s().overlay, VK_ESCAPE);
    assert!(!s().active && s().markup_session.is_none());
    mark_up(&desk);
    // SAFETY: the windows belong to this test.
    unsafe { press(CLOSE) };
    assert!(!s().active);
    assert!(crate::history::list().is_empty(), "nothing was captured");
    // Enter で確定する。すべて消去は、書き込みがあるときだけ元に戻せる操作を残す。
    mark_up(&desk);
    desk.drag(s().overlay, (220, 240), &[], (300, 240));
    unsafe { press(CLEAR) };
    assert_eq!(marks(), 0);
    unsafe { press(CLEAR) };
    assert_eq!(s().markup_session.as_ref().unwrap().undo.len(), 2);
    desk.key(s().overlay, VK_RETURN);
    assert!(!s().active);
    assert_eq!(crate::history::list().len(), 1);
}

mod tray;
