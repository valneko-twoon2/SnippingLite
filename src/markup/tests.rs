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

/// 消しゴムを `at` で押しただけ（道筋が 1 点）のときに触れるか。
fn touches_at(mark: &Mark, at: (f32, f32), reach: f32) -> bool {
    touches(mark, at, at, reach)
}

#[test]
fn the_eraser_reaches_a_line_up_to_its_width_plus_the_reach() {
    let mark = line(&[(0.0, 0.0), (100.0, 0.0)], 2.0);
    // 境界値: 線の太さ 2 と届く距離 8 の合計 10 までは触れる。
    assert!(touches_at(&mark, (50.0, 10.0), 8.0));
    assert!(!touches_at(&mark, (50.0, 10.5), 8.0));
    // 端の先は、端からの距離で決まる。
    assert!(touches_at(&mark, (110.0, 0.0), 8.0));
    assert!(!touches_at(&mark, (110.5, 0.0), 8.0));
    // 1 点だけの線（クリック）と、点のない線。
    assert!(touches_at(&line(&[(5.0, 5.0)], 1.0), (5.0, 12.0), 6.0));
    assert!(!touches_at(&line(&[], 1.0), (0.0, 0.0), 100.0));
    // 同じ点が続いても割り算で壊れない。
    assert!(touches_at(
        &line(&[(3.0, 3.0), (3.0, 3.0)], 1.0),
        (3.0, 4.0),
        0.0
    ));
}

#[test]
fn a_quick_eraser_stroke_erases_a_thin_line_it_jumps_over() {
    // y=0 の線を、(0,-15) と (0,15) の 2 点で横切る。どちらの点も線から 15 離れていて届かないが、
    // 2 点の間の道筋は線の上を通る。
    let mark = line(&[(-50.0, 0.0), (50.0, 0.0)], 2.0);
    assert!(!touches_at(&mark, (0.0, -15.0), 8.0));
    assert!(!touches_at(&mark, (0.0, 15.0), 8.0));
    assert!(touches(&mark, (0.0, -15.0), (0.0, 15.0), 8.0));
    assert!(touches(&mark, (0.0, 15.0), (0.0, -15.0), 8.0), "either way");
    // 境界値: 道筋が線の端から太さ + 届く距離（10）まで近づけば触れ、それより遠ければ触れない。
    assert!(touches(&mark, (60.0, -15.0), (60.0, 15.0), 8.0));
    assert!(!touches(&mark, (60.5, -15.0), (60.5, 15.0), 8.0));
    // 線に沿って動く道筋は、線までの距離で決まる。
    assert!(touches(&mark, (-80.0, 10.0), (80.0, 10.0), 8.0));
    assert!(!touches(&mark, (-80.0, 10.5), (80.0, 10.5), 8.0));
    // 1 点だけの線（クリック）も、横切れば消える。
    assert!(touches(
        &line(&[(0.0, 0.0)], 1.0),
        (-20.0, 3.0),
        (20.0, 3.0),
        2.0
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
    // 範囲がモニターの下端より上で終わるときも、範囲の下端から同じ間隔だけ上（範囲の中）。
    let short = place(rect(500, 50, 900, 990), monitor, 1.0);
    assert_eq!(short.y, 990 - 24 - 48);
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
fn the_bar_stays_off_the_taskbar_when_the_area_is_on_it() {
    // 上のタスクバー（高さ 48）: 作業領域は (0, 48)-(1920, 1080)。タスクバーの上だけを切り取ると、
    // 範囲の下（30 + 12 = 42）はタスクバーにかかるので、作業領域の上端から間隔だけ下（48 + 12）。
    let top = rect(0, 48, 1920, 1080);
    assert_eq!(place(rect(500, 0, 900, 30), top, 1.0).y, 60);
    // 範囲の下端がちょうど作業領域の上端（下端は含まない）でも同じ。
    assert_eq!(place(rect(500, 0, 900, 48), top, 1.0).y, 60);
    // 下のタスクバー: 作業領域は (0, 0)-(1920, 1032)。時計のあたり（1050〜1075）を切り取ると、
    // 範囲の上（1050 - 12 - 48 = 990、下端 1038）はタスクバーにかかるので、1032 - 12 - 48。
    let bottom = rect(0, 0, 1920, 1032);
    assert_eq!(place(rect(1700, 1050, 1900, 1075), bottom, 1.0).y, 972);
    // 範囲が作業領域の中から始まってタスクバーにかかるときは、今までどおり範囲の上。
    assert_eq!(
        place(rect(500, 900, 900, 1075), bottom, 1.0).y,
        900 - 12 - 48
    );
    // 左のタスクバー（幅 48）: 中央に置くと左へはみ出すので、作業領域の左端から間隔だけ右。
    let left = rect(48, 0, 1920, 1080);
    let at = place(rect(0, 500, 40, 600), left, 1.0);
    assert_eq!((at.x, at.y), (60, 612));
    // 右のタスクバー: 作業領域の右端（1872）から間隔とバーの幅だけ左。
    let right = rect(0, 0, 1872, 1080);
    assert_eq!(
        place(rect(1880, 500, 1920, 600), right, 1.0).x,
        1872 - 12 - 432
    );
    // 拡大率 2: 間隔 24、高さ 96。上のタスクバー（96）の上の範囲は、96 + 24 から。
    let top = rect(0, 96, 3840, 2160);
    assert_eq!(place(rect(1000, 0, 1800, 60), top, 2.0).y, 120);
}

#[test]
fn the_bar_takes_the_work_area_and_scale_of_the_monitor_below_the_snip() {
    use crate::ui::monitors::Monitor;
    let monitor = |r: RECT, taskbar: i32, dpi: u32| Monitor {
        rect: r,
        work: RECT {
            bottom: r.bottom - taskbar,
            ..r
        },
        dpi,
    };
    // 主モニター A（100%）、右に D（2560x1440、150%）、左に B（負の座標、125%）。
    let a = monitor(rect(0, 0, 1920, 1080), 48, 96);
    let d = monitor(rect(1920, 0, 4480, 1440), 72, 144);
    let b = monitor(rect(-1920, 0, 0, 1080), 60, 120);
    let edges =
        |(work, scale): (RECT, f32)| ((work.left, work.top, work.right, work.bottom), scale);
    // 境目をまたぐ範囲は、下端の中央の点のモニター: (2000, 600) は D、(1725, 600) は A。
    let across = home(rect(1800, 300, 2200, 600), &[a, d]).unwrap();
    assert_eq!(edges(across), ((1920, 0, 4480, 1368), 1.5));
    let left_part = home(rect(1500, 300, 1950, 600), &[a, d]).unwrap();
    assert_eq!(edges(left_part), ((0, 0, 1920, 1032), 1.0));
    // 画面の下端までの範囲: 下端の中央 (1000, 1080) はすぐ上の A がいちばん近い。
    let bottom = home(rect(800, 500, 1200, 1080), &[a, d]).unwrap();
    assert_eq!(edges(bottom), ((0, 0, 1920, 1032), 1.0));
    // 負の座標の左のモニターの上では、そこの 125% で、その作業領域の中に置く。
    let area = rect(-600, 100, -200, 400);
    let (work, scale) = home(area, &[a, b]).unwrap();
    assert_eq!(edges((work, scale)), ((-1920, 0, 0, 1020), 1.25));
    // 幅 540、高さ 60、間隔 15: 中央の -400 から 270 左、範囲の下端 400 から 15 下。
    let at = place(area, work, scale);
    assert_eq!((at.x, at.y), (-670, 415));
    assert!(home(rect(0, 0, 1, 1), &[]).is_none());
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

/// スクリーンショットの保存先にできたファイル。
fn shots(desk: &Desk) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(desk.sandbox.path().join("shots"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|f| f.path())
        .collect();
    files.sort();
    files
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

mod keys;
mod model;
mod modes;
mod properties;
mod tray;
