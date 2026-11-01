//! ボタンのヒント。
//!
//! | 場面 | ヒント | テスト |
//! |---|---|---|
//! | ボタンに置いて待つ | 待ちが明けてからボタンの下に出す | `a_tip_waits_then_shows_the_name_below_the_button` |
//! | 出している間に隣のボタンへ | すぐ隠し、短い待ちで隣の名前を出す | 同上 |
//! | ボタンの外・ヒントのないボタン | 出さない | 同上 |
//! | マウスが離れる・押す | 隠し、待ちも取りやめる | `leaving_or_pressing_hides_the_tip` |
//! | ボタンを持つ窓が隠れる | 隠す。待ちが明けても出さない | `a_hidden_bar_shows_no_tip` |
//! | 書き込みのバー | アイコンのボタンにだけ出す | `the_markup_bar_names_its_icon_buttons` |
//! | 置く所 | 下に入らなければ上。左右は作業領域の中 | `a_tip_goes_below_the_button_unless_it_would_leave_the_work_area` |

use super::*;
use crate::test_support::desk::Desk;
use std::time::Duration;

fn s() -> &'static mut State {
    // SAFETY: the test owns the state and runs on the thread that created it.
    unsafe { state() }
}

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT {
        left,
        top,
        right,
        bottom,
    }
}

fn shown_now() -> bool {
    // SAFETY: plain window query.
    unsafe { shown() }
}

fn window_rect(window: HWND) -> RECT {
    let mut bounds = rect(0, 0, 0, 0);
    // SAFETY: plain window query.
    unsafe { GetWindowRect(window, &mut bounds) };
    bounds
}

/// ツールバーのボタンの真ん中へマウスを動かす。
fn hover_toolbar(desk: &Desk, index: i32) {
    let [l, t, r, b] = toolbar_button(index, s().scale);
    desk.mouse(
        s().toolbar,
        WM_MOUSEMOVE,
        ((l + r) / 2.0) as i32,
        ((t + b) / 2.0) as i32,
    );
}

/// 待ちが明けたことにする。実際のマウスはツールバーの上にないので、待つ間にメッセージを回すと
/// マウスが離れた知らせが先に届く。
fn elapse(desk: &Desk) {
    desk.send(s().tip.window, WM_TIMER, SHOW_TIMER, 0);
}

fn start(desk: &Desk) {
    // SAFETY: the windows belong to this test.
    unsafe { begin_capture() };
    assert!(s().active);
    desk.pump(Duration::from_millis(20));
}

#[test]
fn a_tip_goes_below_the_button_unless_it_would_leave_the_work_area() {
    let work = rect(0, 0, 1920, 1040);
    // 下に入れば、ボタンの下にボタンと中央をそろえて置く。
    let below = place(rect(100, 20, 136, 52), (60, 28), work, 1.0);
    assert_eq!((below.x, below.y), (88, 64));
    // 倍率に合わせて離す。
    let scaled = place(rect(100, 20, 136, 52), (60, 28), work, 2.0);
    assert_eq!(scaled.y, 52 + 24);
    // 境界値: 下端にちょうど入るときは下、1 画素でもはみ出すときは上。
    let fits = place(rect(100, 988, 136, 1000), (60, 28), work, 1.0);
    assert_eq!(fits.y, 1012);
    let over = place(rect(100, 989, 136, 1001), (60, 28), work, 1.0);
    assert_eq!(over.y, 989 - 12 - 28);
    // 左右の端では作業領域の中へ寄せる。
    assert_eq!(place(rect(0, 20, 20, 52), (60, 28), work, 1.0).x, 0);
    assert_eq!(place(rect(1900, 20, 1920, 52), (60, 28), work, 1.0).x, 1860);
    // 負の座標のモニターでも同じ。
    let left = rect(-1920, 0, 0, 1040);
    assert_eq!(
        place(rect(-1920, 20, -1900, 52), (60, 28), left, 1.0).x,
        -1920
    );
}

#[test]
#[ignore = "Covers the screen"]
fn a_tip_waits_then_shows_the_name_below_the_button() {
    for dark in [false, true] {
        let desk = Desk::start();
        start(&desk);
        s().theme.dark = dark;
        hover_toolbar(&desk, HISTORY_BUTTON);
        // すぐには出さない。
        assert!(!shown_now());
        // SAFETY: the state belongs to this test.
        assert_eq!(unsafe { text() }, tr("History"));
        elapse(&desk);
        assert!(shown_now(), "the tip shows once the wait is over");
        let (tip, bar) = (window_rect(s().tip.window), window_rect(s().toolbar));
        assert!(
            tip.top >= bar.bottom,
            "the tip at {} is below the toolbar ending at {}",
            tip.top,
            bar.bottom
        );
        let [l, _, r, _] = toolbar_button(HISTORY_BUTTON, s().scale);
        let middle = bar.left + ((l + r) / 2.0) as i32;
        assert!(
            ((tip.left + tip.right) / 2 - middle).abs() <= 1,
            "the tip from {} to {} is centered on {middle}",
            tip.left,
            tip.right
        );
        desk.send(s().tip.window, WM_PAINT, 0, 0);
        // 出している間に隣のボタンへ移ると、いったん隠して隣の名前を出す。
        hover_toolbar(&desk, SETTINGS_BUTTON);
        assert!(!shown_now());
        // SAFETY: the state belongs to this test.
        assert_eq!(unsafe { text() }, tr("Settings"));
        elapse(&desk);
        assert!(shown_now());
        // ボタンの間（ツールバーを動かす所）には出さない。
        desk.mouse(s().toolbar, WM_MOUSEMOVE, 86, 24);
        assert!(!shown_now());
        elapse(&desk);
        assert!(!shown_now(), "nothing to show between the buttons");
        desk.key(s().overlay, 0x1b);
    }
}

#[test]
#[ignore = "Covers the screen"]
fn leaving_or_pressing_hides_the_tip() {
    let desk = Desk::start();
    start(&desk);
    hover_toolbar(&desk, CLOSE_BUTTON);
    elapse(&desk);
    assert!(shown_now());
    desk.send(s().toolbar, WM_MOUSELEAVE, 0, 0);
    assert!(!shown_now(), "leaving the toolbar hides the tip");
    // 押したら、離れてまた来るまで出さない。
    hover_toolbar(&desk, EDIT_BUTTON);
    let [l, t, r, b] = toolbar_button(EDIT_BUTTON, s().scale);
    let (x, y) = (((l + r) / 2.0) as i32, ((t + b) / 2.0) as i32);
    desk.mouse(s().toolbar, WM_LBUTTONDOWN, x, y);
    elapse(&desk);
    assert!(!shown_now(), "a press cancels the tip");
    desk.mouse(s().toolbar, WM_LBUTTONUP, x, y);
    // マウスはヒントの窓を通り抜け、ヒントの窓は前面に出ない。
    assert_eq!(
        desk.send(s().tip.window, WM_NCHITTEST, 0, 0),
        HTTRANSPARENT as LRESULT
    );
    assert_eq!(
        desk.send(s().tip.window, WM_MOUSEACTIVATE, 0, 0),
        MA_NOACTIVATE as LRESULT
    );
    assert_eq!(desk.send(s().tip.window, WM_ERASEBKGND, 0, 0), 1);
    desk.key(s().overlay, 0x1b);
}

#[test]
#[ignore = "Covers the screen"]
fn a_hidden_bar_shows_no_tip() {
    let desk = Desk::start();
    start(&desk);
    // 出しているヒントは、ツールバーとともに隠れる。
    hover_toolbar(&desk, HISTORY_BUTTON);
    elapse(&desk);
    assert!(shown_now());
    desk.key(s().overlay, 0x1b);
    assert!(
        !s().active && !shown_now(),
        "the tip leaves with the toolbar"
    );
    // 待っている間にツールバーが隠れたら、待ちが明けても出さない。
    start(&desk);
    hover_toolbar(&desk, HISTORY_BUTTON);
    // SAFETY: the window belongs to this test.
    unsafe { ShowWindow(s().toolbar, SW_HIDE) };
    s().tip.text = tr("History");
    elapse(&desk);
    assert!(!shown_now(), "no tip for a hidden toolbar");
    desk.key(s().overlay, 0x1b);
}

#[test]
#[ignore = "Covers the screen"]
fn the_markup_bar_names_its_icon_buttons() {
    let desk = Desk::start();
    s().markup = true;
    start(&desk);
    desk.drag(s().overlay, (200, 200), &[], (400, 320));
    assert!(s().markup_session.is_some(), "marking up");
    let bar = s().markup_bar;
    let at = |index: i32| {
        let [l, t, r, b] = markup::probe::button(index, s().scale);
        (((l + r) / 2.0) as i32, ((t + b) / 2.0) as i32)
    };
    for (index, name) in [
        (markup::probe::PEN, tr("Pen")),
        (markup::probe::ERASER, tr("Eraser")),
        (markup::probe::CLEAR, tr("Clear all")),
    ] {
        let (x, y) = at(index);
        desk.mouse(bar, WM_MOUSEMOVE, x, y);
        // SAFETY: the state belongs to this test.
        assert_eq!(unsafe { text() }, name);
        elapse(&desk);
        assert!(shown_now(), "{name}");
    }
    // 「キャプチャ」は名前が書いてあるので出さない。
    let (x, y) = at(markup::probe::CAPTURE);
    desk.mouse(bar, WM_MOUSEMOVE, x, y);
    assert!(!shown_now());
    // SAFETY: the state belongs to this test.
    assert_eq!(unsafe { text() }, "");
    // 書き込みを終えてバーが隠れると、ヒントも隠れる。
    let (x, y) = at(markup::probe::REDO);
    desk.mouse(bar, WM_MOUSEMOVE, x, y);
    elapse(&desk);
    assert!(shown_now());
    desk.key(s().overlay, 0x1b);
    assert!(!shown_now(), "the tip leaves with the markup bar");
}
