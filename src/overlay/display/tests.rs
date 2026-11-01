//! 合成した WM_DISPLAYCHANGE を本物の窓に送り、規則（display.rs の表、README）どおりに応えるかを
//! 確かめる。この PC の構成は変えない。覚えてある並びを偽物に替えてから送ると、届いた後に確かめる
//! 今の並びと食い違うので、構成が変わったことになる。
//!
//! | 状態 | 並び | 期待 | テスト |
//! |---|---|---|---|
//! | 範囲を選んでいる（静止・ライブ） | 同じ | 続ける。選んでいる範囲もそのまま | `the_same_layout_keeps_the_snip_and_its_selection` |
//! | 範囲を選んでいる（静止・ライブ） | 変わった | 知らせを流した後に取り消す。残す設定のツールバーは画面の上に出る | `a_new_layout_cancels_the_snip_after_the_system_message` |
//! | 書き込み中 | 変わった | 続ける。描いた線は残り、バーは作業領域の中 | `marking_up_survives_a_new_layout_and_its_bar_comes_back_on_screen` |
//! | 残したツールバー | 変わった | 画面の上ならそのまま、外ならいちばん近いモニターの中 | `a_kept_toolbar_comes_back_onto_a_monitor_only_when_it_was_pushed_off` |
//! | カウントダウン | 範囲の下が同じ / 変わった | 続ける / 取り消す | `recording::a_countdown_goes_on_unless_the_monitor_under_its_area_changed` |
//! | 録画中 | 範囲の下が同じ / 変わった | 続けて表示を置き直す / 止めて保存する | `recording::a_recording_stops_and_is_saved_only_when_the_monitor_under_it_changed` |
//! | ウィンドウのカウントダウンと録画 | 窓の下が変わった | どちらも続ける（窓を追いかける） | `recording::a_window_countdown_and_recording_follow_the_window_through_a_new_layout` |
//! | 通知 | 変わった | 作業領域の右下に戻る | `a_notification_returns_to_the_lower_right_of_the_work_area` |

use super::*;
use crate::test_support::desk::Desk;
use std::time::Duration;

mod dpi;
mod recording;

fn s() -> &'static mut State {
    // SAFETY: the test owns the state and runs on the thread that created it.
    unsafe { state() }
}

fn shown(window: HWND) -> bool {
    // SAFETY: plain window query.
    !window.is_null() && unsafe { IsWindowVisible(window) } != 0
}

fn bounds(window: HWND) -> RECT {
    // SAFETY: plain window query.
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        GetWindowRect(window, &mut rect);
        rect
    }
}

fn edges(r: RECT) -> (i32, i32, i32, i32) {
    (r.left, r.top, r.right, r.bottom)
}

fn lparam(x: i32, y: i32) -> LPARAM {
    ((y as u16 as isize) << 16) | (x as u16 as isize)
}

/// Windows が選ぶ、矩形にいちばん近いモニターの画面全体と作業領域。
fn monitor_of(r: RECT) -> (RECT, RECT) {
    // SAFETY: plain monitor queries with valid out parameters.
    unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST), &mut info);
        (info.rcMonitor, info.rcWork)
    }
}

fn now() -> Vec<Monitor> {
    // SAFETY: plain monitor queries.
    unsafe { monitors::current() }
}

/// 今の並びで、点 `p`（画面の座標）のあるモニターを左半分の幅にしたもの（そのモニターの解像度を
/// 変える前の並びの代わり）。点がどのモニターにもなければ最初のモニター。
fn narrower_at(p: POINT) -> Vec<Monitor> {
    let mut list = now();
    let index = list
        .iter()
        .position(|m| {
            p.x >= m.rect.left && p.x < m.rect.right && p.y >= m.rect.top && p.y < m.rect.bottom
        })
        .unwrap_or(0);
    let r = &mut list[index].rect;
    r.right -= (r.right - r.left) / 2;
    list
}

/// 主モニターの解像度を変える前の並びの代わり。
fn narrower() -> Vec<Monitor> {
    narrower_at(POINT { x: 0, y: 0 })
}

/// 選んだ範囲 `AREA` の左上（画面の座標）。
fn area_corner() -> POINT {
    POINT {
        x: s().origin.x + AREA.left,
        y: s().origin.y + AREA.top,
    }
}

/// 今の並びに、仮想画面の左の外にもう 1 台足したもの（そのモニターを外す前の並びの代わり）。
fn with_one_more_on_the_left() -> Vec<Monitor> {
    let mut list = now();
    // SAFETY: plain metric queries.
    let (left, top) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        )
    };
    let rect = RECT {
        left: left - 1920,
        top,
        right: left,
        bottom: top + 1080,
    };
    list.push(Monitor {
        rect,
        work: rect,
        dpi: 120,
        device: ui::monitors::device(r"\\.\GONE"),
    });
    list
}

/// システムが送るように WM_DISPLAYCHANGE をすべての窓に送り（色数 32、解像度は知らせるだけ）、
/// 積まれた知らせを流す。本物のマウスとキーボードの入力は捨てる（範囲を選んでいる間は
/// オーバーレイがマウスをつかんでいるので、本物のポインターの位置が届いてしまう）。
fn display_change(desk: &Desk) {
    for window in [desk.main, s().overlay, s().toolbar] {
        desk.send(window, WM_DISPLAYCHANGE, 32, lparam(1920, 1080));
    }
    flush();
}

/// たまったメッセージを流す。本物のマウスとキーボードの入力は捨てる。
fn flush() {
    let end = std::time::Instant::now() + Duration::from_millis(30);
    while std::time::Instant::now() < end {
        // SAFETY: plain message loop calls on this thread's queue.
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                let input = matches!(
                    message.message,
                    WM_MOUSEFIRST..=WM_MOUSELAST | WM_KEYFIRST..=WM_KEYLAST
                );
                if !input {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// 撮影を始める。開始時に設定ファイルを読み直すので、ファイルに書いて切り替える。
fn start(desk: &Desk, freeze: bool) {
    let mut settings = config::load_settings();
    settings.freeze = freeze;
    config::save_settings(&settings).unwrap();
    // SAFETY: the windows belong to this test.
    unsafe { begin_capture() };
    assert!(s().active, "the snip started");
    desk.pump(Duration::from_millis(20));
}

/// 範囲を選んでいる途中まで（押して動かしたが、離していない）。
fn half_drag(desk: &Desk) {
    desk.mouse(s().overlay, WM_LBUTTONDOWN, 100, 100);
    desk.mouse(s().overlay, WM_MOUSEMOVE, 260, 200);
    assert!(s().dragging);
}

#[test]
#[ignore = "Covers the screen"]
fn the_same_layout_keeps_the_snip_and_its_selection() {
    let desk = Desk::start();
    for freeze in [true, false] {
        start(&desk, freeze);
        half_drag(&desk);
        display_change(&desk);
        assert!(
            s().active && shown(s().overlay),
            "freeze {freeze}: the snip goes on"
        );
        assert!(s().dragging, "freeze {freeze}: and so does the selection");
        assert_eq!(
            (s().start.x, s().start.y, s().end.x, s().end.y),
            (100, 100, 260, 200)
        );
        desk.key(s().overlay, 0x1b);
        assert!(!s().active);
    }
}

#[test]
#[ignore = "Covers the screen"]
fn a_new_layout_cancels_the_snip_after_the_system_message() {
    let desk = Desk::start_with(|settings| {
        settings.keep_toolbar = true;
        settings.history = true;
    });
    for freeze in [true, false] {
        start(&desk, freeze);
        half_drag(&desk);
        remember(narrower());
        desk.send(s().overlay, WM_DISPLAYCHANGE, 32, lparam(960, 1080));
        // 知らせの処理の中では何もしない（確かめるのは、それを抜けてから）。
        assert!(
            s().active,
            "freeze {freeze}: checked after the system message"
        );
        flush();
        assert!(
            !s().active && !shown(s().overlay),
            "freeze {freeze}: cancelled"
        );
        assert!(!s().dragging && s().points.is_empty());
        assert!(
            s().dim_canvas.is_none() && s().live.is_none() && s().screen.is_null(),
            "freeze {freeze}: the stale copy of the screen is gone"
        );
        // 何も届かない（取り消しは Escape と同じ）。
        assert!(history::list().is_empty(), "freeze {freeze}");
        // 残す設定のツールバーは、待機中に戻って画面の上に出る。
        assert!(s().toolbar_open && shown(s().toolbar), "freeze {freeze}");
        let toolbar = bounds(s().toolbar);
        assert!(
            ui::monitors::covered(&now(), toolbar),
            "freeze {freeze}: the toolbar {:?} is on a monitor",
            edges(toolbar)
        );
    }
}

#[test]
#[ignore = "Covers the screen"]
fn marking_up_survives_a_new_layout_and_its_bar_comes_back_on_screen() {
    let desk = Desk::start_with(|settings| {
        settings.copy = false;
        settings.history = true;
        settings.notify = false;
    });
    s().markup = true;
    start(&desk, true);
    desk.drag(s().overlay, (200, 200), &[], (500, 400));
    assert!(s().markup_session.is_some(), "marking up");
    desk.drag(s().overlay, (250, 250), &[(300, 280)], (350, 300));
    // バーを画面の外へやる（バーのあったモニターを外した後の代わり）。
    // SAFETY: the window belongs to this test.
    unsafe {
        let right = GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN);
        SetWindowPos(
            s().markup_bar,
            null_mut(),
            right + 500,
            100,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    remember(narrower());
    display_change(&desk);
    assert!(s().active, "the drawing is not thrown away");
    // SAFETY: the state belongs to this test.
    let look = unsafe { markup::probe::look() }.expect("still marking up");
    assert_eq!(look.marks, 1, "the line is still there");
    assert!(shown(s().markup_bar) && !shown(s().toolbar));
    let bar = bounds(s().markup_bar);
    let (_, work) = monitor_of(bar);
    assert!(
        bar.left >= work.left
            && bar.top >= work.top
            && bar.right <= work.right
            && bar.bottom <= work.bottom,
        "the bar {:?} is inside {:?}",
        edges(bar),
        edges(work)
    );
    // 確定すれば、線ごと届く。
    desk.key(s().overlay, VK_RETURN);
    assert!(!s().active);
    assert_eq!(history::list().len(), 1);
}

#[test]
#[ignore = "Covers the screen"]
fn a_kept_toolbar_comes_back_onto_a_monitor_only_when_it_was_pushed_off() {
    let desk = Desk::start_with(|settings| settings.keep_toolbar = true);
    start(&desk, true);
    desk.key(s().overlay, 0x1b);
    assert!(s().toolbar_open && shown(s().toolbar), "kept between snips");
    let move_to = |x: i32, y: i32| {
        // SAFETY: the window belongs to this test.
        unsafe {
            SetWindowPos(
                s().toolbar,
                null_mut(),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
    };
    // 画面の上なら、置いた所を守る（README: × で閉じるまで、動かした所に出る）。
    move_to(300, 80);
    remember(narrower());
    display_change(&desk);
    assert_eq!(
        (bounds(s().toolbar).left, bounds(s().toolbar).top),
        (300, 80)
    );
    // 仮想画面の右の外（モニターを外した後の代わり）: いちばん近いモニターの右端の内側へ。
    // SAFETY: plain metric queries.
    let right =
        unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    move_to(right + 100, 50);
    let off = bounds(s().toolbar);
    let (monitor, _) = monitor_of(off);
    let width = off.right - off.left;
    let height = off.bottom - off.top;
    remember(narrower());
    display_change(&desk);
    let back = bounds(s().toolbar);
    let expected = (
        (monitor.right - width).max(monitor.left),
        50.min(monitor.bottom - height).max(monitor.top),
    );
    assert_eq!((back.left, back.top), expected, "the nearest monitor");
    assert!(shown(s().toolbar) && !s().active);
}

/// 録画のスナップを始め、`area` を選ぶ。カウントダウンがあれば、その 1 秒のタイマーは止める。
fn select_recording(desk: &Desk, area: RECT) {
    s().video = true;
    start(desk, true);
    desk.drag(
        s().overlay,
        (area.left, area.top),
        &[],
        (area.right, area.bottom),
    );
    if !s().count_window.is_null() {
        // SAFETY: the window belongs to this test.
        unsafe { KillTimer(s().count_window, COUNT_TIMER) };
    }
}

const AREA: RECT = RECT {
    left: 200,
    top: 200,
    right: 400,
    bottom: 320,
};

#[test]
#[ignore = "Shows notification windows"]
fn a_notification_returns_to_the_lower_right_of_the_work_area() {
    let desk = Desk::start();
    // SAFETY: the windows belong to this test.
    unsafe { toast::show(&[0; 4], 1, 1, true, None) };
    let look = unsafe { toast::probe::look() }.expect("a notification");
    // 主モニターの作業領域の右下に、端から 12 DIP 空けて、幅 364 DIP、高さ 136 DIP（本文なし）。
    let mut work: RECT = unsafe { std::mem::zeroed() };
    unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0) };
    let at = |v: f32| (v * look.scale).round() as i32;
    let expected = (
        work.right - at(12.0) - at(364.0),
        work.bottom - at(12.0) - at(136.0),
        work.right - at(12.0),
        work.bottom - at(12.0),
    );
    assert!(
        desk.pump_until(Duration::from_secs(3), || edges(bounds(look.hwnd))
            == expected),
        "it slides into the lower right"
    );
    // 滑り終えるまで待つ（滑っている途中なら、横の位置は次の刻みで動く）。
    desk.pump(Duration::from_millis(300));
    // ずれた所（構成が変わる前の作業領域の代わり）から、構成が変わると右下へ戻る。
    // SAFETY: the window belongs to this test.
    unsafe {
        SetWindowPos(
            look.hwnd,
            null_mut(),
            work.left,
            work.top,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
    remember(narrower());
    display_change(&desk);
    assert_eq!(edges(bounds(look.hwnd)), expected);
    assert!(unsafe { toast::visible() }, "and stays until it times out");
}
