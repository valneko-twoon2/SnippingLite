//! 合成した WM_DPICHANGED を本物の窓に送り、新しい倍率で作り直すかを確かめる。この PC の拡大率は
//! 変えない。期待する大きさは DIP の寸法（ツールバー 332x48、バー 432x48、カウントダウン 120、
//! 録画中の表示 168x48、通知 364x136）に倍率を掛けて手で求める。
//!
//! | 窓 | 期待 | テスト |
//! |---|---|---|
//! | オーバーレイ | 動かさず、撮影も選んでいる範囲も続く | `the_overlay_keeps_covering_the_screen_when_the_dpi_changes` |
//! | ツールバー | 提案された矩形の左上に、新しい倍率の大きさとボタンで | `the_toolbar_is_rebuilt_at_the_new_scale_in_the_suggested_place` |
//! | 書き込みのバー | 範囲の下に、新しい倍率で | `the_markup_bar_is_rebuilt_at_the_new_scale_below_the_snip` |
//! | 隠れたツールバー | 何もしない（共有の倍率を書き換えず、見えている窓に任せる） | `a_hidden_toolbar_leaves_the_scale_to_the_markup_bar` |
//! | カウントダウン | 範囲の真ん中に、新しい倍率で | `the_countdown_is_rebuilt_at_the_new_scale_over_the_middle_of_its_area` |
//! | 録画中の表示 | 上部中央に、新しい倍率の大きさと停止のボタンで | `the_recording_indicator_is_rebuilt_at_the_new_scale_at_the_top_center` |
//! | 通知 | 作業領域の右下に、新しい倍率で | `the_notification_is_rebuilt_at_the_new_scale_in_the_lower_right` |
//!
//! どの窓も、倍率が今と同じ知らせ（自分で置き直したときに届くもの）では動かない。

use super::*;

/// WM_DPICHANGED を送る。wParam は上位に縦、下位に横の DPI、lParam は提案する矩形。
fn dpi_changed(desk: &Desk, window: HWND, dpi: u32, suggested: RECT) -> LRESULT {
    let wp = ((dpi as usize) << 16) | dpi as usize;
    desk.send(
        window,
        WM_DPICHANGED,
        wp,
        &suggested as *const RECT as LPARAM,
    )
}

/// 今の倍率と違う DPI（125%、150%、200% の順に試す）。
fn other_dpis() -> Vec<u32> {
    [120, 144, 192]
        .into_iter()
        .filter(|&dpi| dpi as f32 / 96.0 != s().scale)
        .collect()
}

fn rect(left: i32, top: i32, width: i32, height: i32) -> RECT {
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

fn dip(value: f32, scale: f32) -> i32 {
    (value * scale).round() as i32
}

#[test]
#[ignore = "Covers the screen"]
fn the_overlay_keeps_covering_the_screen_when_the_dpi_changes() {
    let desk = Desk::start();
    start(&desk, true);
    half_drag(&desk);
    let (before, scale) = (bounds(s().overlay), s().scale);
    let half = rect(0, 0, (before.right - before.left) / 2, 300);
    assert_eq!(dpi_changed(&desk, s().overlay, 144, half), 0);
    assert_eq!(edges(bounds(s().overlay)), edges(before), "not moved");
    assert!(
        s().active && s().dragging,
        "the snip and the selection go on"
    );
    assert_eq!(s().scale, scale, "the toolbar keeps its own scale");
    desk.key(s().overlay, 0x1b);
}

#[test]
#[ignore = "Covers the screen"]
fn the_toolbar_is_rebuilt_at_the_new_scale_in_the_suggested_place() {
    let desk = Desk::start();
    start(&desk, true);
    let toolbar = s().toolbar;
    for (step, dpi) in other_dpis().into_iter().enumerate() {
        let scale = dpi as f32 / 96.0;
        let (left, top) = (100 + step as i32 * 10, 50);
        // 提案の大きさはわざと違えておく: 大きさは倍率から決める。
        dpi_changed(&desk, toolbar, dpi, rect(left, top, 10, 10));
        assert_eq!(s().scale, scale, "{dpi}");
        assert_eq!(
            edges(bounds(toolbar)),
            edges(rect(left, top, dip(332.0, scale), dip(48.0, scale))),
            "{dpi}"
        );
        // ボタンも新しい倍率の所にある: × の真ん中（306, 24 DIP）はボタン、区切りの隙間
        // （86 DIP）はつかんで動かす所。
        let screen = |x: f32| lparam(left + dip(x, scale), top + dip(24.0, scale));
        assert_eq!(
            desk.send(toolbar, WM_NCHITTEST, 0, screen(306.0)),
            HTCLIENT as LRESULT
        );
        assert_eq!(
            desk.send(toolbar, WM_NCHITTEST, 0, screen(86.0)),
            HTCAPTION as LRESULT
        );
        desk.mouse(toolbar, WM_MOUSEMOVE, dip(306.0, scale), dip(24.0, scale));
        assert_eq!(s().hover_button, CLOSE_BUTTON, "{dpi}");
        desk.send(toolbar, WM_MOUSELEAVE, 0, 0);
        desk.send(toolbar, WM_PAINT, 0, 0);
        // 同じ倍率の知らせ（自分で置き直したとき）では動かない。
        dpi_changed(&desk, toolbar, dpi, rect(700, 700, 10, 10));
        assert_eq!((bounds(toolbar).left, bounds(toolbar).top), (left, top));
    }
    assert!(s().active);
    desk.key(s().overlay, 0x1b);
}

#[test]
#[ignore = "Covers the screen"]
fn the_markup_bar_is_rebuilt_at_the_new_scale_below_the_snip() {
    let desk = Desk::start_with(|settings| settings.copy = false);
    s().markup = true;
    start(&desk, true);
    desk.drag(s().overlay, (200, 200), &[], (500, 400));
    let bar = s().markup_bar;
    assert!(shown(bar), "marking up");
    let origin = s().origin;
    // 範囲の下端の中央の点のモニター。
    let anchor = rect(origin.x + 350, origin.y + 400, 1, 1);
    let (_, work) = monitor_of(anchor);
    for dpi in other_dpis() {
        let scale = dpi as f32 / 96.0;
        dpi_changed(&desk, bar, dpi, rect(0, 0, 10, 10));
        assert_eq!(s().scale, scale, "{dpi}");
        // 範囲の下の中央に、作業領域の端から 12 DIP の内側で（markup::bar::place の規則）。
        let (width, height, gap) = (dip(432.0, scale), dip(48.0, scale), dip(12.0, scale));
        let x = (origin.x + 350 - width / 2)
            .min(work.right - width - gap)
            .max(work.left + gap);
        let below = origin.y + 400 + gap;
        assert!(below + height <= work.bottom - gap, "there is room below");
        assert_eq!(
            edges(bounds(bar)),
            edges(rect(x, below, width, height)),
            "{dpi}"
        );
        // 消しゴムのボタン（200〜236 DIP）を新しい倍率の所で押すと、消しゴムになる。
        let (ex, ey) = (dip(218.0, scale), dip(24.0, scale));
        desk.click(bar, ex, ey);
        let look = unsafe { markup::probe::look() }.expect("still marking up");
        assert_eq!(look.tool, markup::Tool::Eraser, "{dpi}");
        desk.click(bar, dip(138.0, scale), ey);
        // 同じ倍率の知らせでは動かない。
        let before = bounds(bar);
        dpi_changed(&desk, bar, dpi, rect(900, 900, 10, 10));
        assert_eq!(edges(bounds(bar)), edges(before));
    }
    desk.key(s().overlay, 0x1b);
    assert!(!s().active);
}

#[test]
#[ignore = "Covers the screen"]
fn a_hidden_toolbar_leaves_the_scale_to_the_markup_bar() {
    let desk = Desk::start_with(|settings| settings.copy = false);
    s().markup = true;
    start(&desk, true);
    desk.drag(s().overlay, (200, 200), &[], (500, 400));
    let (bar, toolbar) = (s().markup_bar, s().toolbar);
    assert!(
        shown(bar) && !shown(toolbar),
        "marking up hides the toolbar"
    );
    let dpi = other_dpis()[0];
    let scale = dpi as f32 / 96.0;
    // 隠れたツールバーが先に受け取っても、倍率は書き込みのバーのものから変わらない。
    let before = s().scale;
    dpi_changed(&desk, toolbar, dpi, rect(100, 50, 10, 10));
    assert_eq!(
        s().scale,
        before,
        "the hidden toolbar does not take the scale"
    );
    // 後から受け取った書き込みのバーは、新しい倍率の大きさ（432x48 DIP）で作り直される。
    dpi_changed(&desk, bar, dpi, rect(0, 0, 10, 10));
    assert_eq!(s().scale, scale);
    let r = bounds(bar);
    assert_eq!(
        (r.right - r.left, r.bottom - r.top),
        (dip(432.0, scale), dip(48.0, scale)),
        "the bar is as wide as its buttons"
    );
    desk.key(s().overlay, 0x1b);
    assert!(!s().active);
}

#[test]
#[ignore = "Covers the screen"]
fn the_countdown_is_rebuilt_at_the_new_scale_over_the_middle_of_its_area() {
    let desk = Desk::start_with(|settings| {
        settings.countdown = true;
        settings.system_audio = false;
    });
    select_recording(&desk, AREA);
    let count = s().count_window;
    assert!(!count.is_null(), "counting down");
    let origin = s().origin;
    // 範囲 (200, 200)〜(400, 320) の真ん中は (300, 260)。
    let (cx, cy) = (origin.x + 300, origin.y + 260);
    for dpi in other_dpis() {
        let scale = dpi as f32 / 96.0;
        dpi_changed(&desk, count, dpi, rect(0, 0, 10, 10));
        assert_eq!(s().scale, scale, "{dpi}");
        let size = dip(120.0, scale);
        assert_eq!(
            edges(bounds(count)),
            edges(rect(cx - size / 2, cy - size / 2, size, size)),
            "{dpi}"
        );
        desk.send(count, WM_PAINT, 0, 0);
        let before = bounds(count);
        dpi_changed(&desk, count, dpi, rect(0, 0, 10, 10));
        assert_eq!(edges(bounds(count)), edges(before));
    }
    assert_eq!(s().count_left, 3, "the countdown is not restarted");
    desk.mouse(count, WM_LBUTTONUP, 10, 10);
    assert!(s().count_window.is_null() && s().recording.is_none());
}

#[test]
#[ignore = "Records the screen"]
fn the_recording_indicator_is_rebuilt_at_the_new_scale_at_the_top_center() {
    let desk = Desk::start_with(|settings| {
        settings.countdown = false;
        settings.system_audio = false;
        settings.notify = false;
    });
    select_recording(&desk, AREA);
    let indicator = s().record_window;
    assert!(s().recording.is_some() && !indicator.is_null());
    let origin = s().origin;
    let (monitor, _) = monitor_of(rect(origin.x + 300, origin.y + 260, 1, 1));
    let dpi = other_dpis()[0];
    let scale = dpi as f32 / 96.0;
    dpi_changed(&desk, indicator, dpi, rect(0, 0, 10, 10));
    assert_eq!(s().scale, scale);
    let width = dip(168.0, scale);
    let left = (monitor.left + monitor.right - width) / 2;
    assert_eq!(
        edges(bounds(indicator)),
        edges(rect(
            left,
            monitor.top + dip(12.0, scale),
            width,
            dip(48.0, scale)
        )),
        "the top center"
    );
    desk.send(indicator, WM_PAINT, 0, 0);
    // 停止のボタン（100〜160 DIP）も新しい倍率の所にある: 経過時間（60 DIP）では止まらない。
    desk.click(indicator, dip(60.0, scale), dip(24.0, scale));
    assert!(s().recording.is_some());
    desk.click(indicator, dip(130.0, scale), dip(24.0, scale));
    assert!(s().recording.is_none(), "the Stop button ends it");
    let videos = std::fs::read_dir(desk.sandbox.path().join("videos"))
        .unwrap()
        .count();
    assert_eq!(videos, 1);
}

#[test]
#[ignore = "Shows notification windows"]
fn the_notification_is_rebuilt_at_the_new_scale_in_the_lower_right() {
    let desk = Desk::start();
    // SAFETY: the windows belong to this test.
    unsafe { toast::show(&[0; 4], 1, 1, true, None) };
    let hwnd = unsafe { toast::probe::look() }
        .expect("a notification")
        .hwnd;
    let mut work: RECT = unsafe { std::mem::zeroed() };
    unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0) };
    let first = unsafe { toast::probe::look() }.unwrap().scale;
    let resting = |scale: f32| {
        let (width, height, margin) = (dip(364.0, scale), dip(136.0, scale), dip(12.0, scale));
        edges(rect(
            work.right - margin - width,
            work.bottom - margin - height,
            width,
            height,
        ))
    };
    assert!(
        desk.pump_until(Duration::from_secs(3), || edges(bounds(hwnd))
            == resting(first)),
        "it slides in"
    );
    // 滑り終えるまで待つ（滑っている途中なら、横の位置は次の刻みで動く）。
    desk.pump(Duration::from_millis(300));
    for dpi in [120, 144, 192]
        .into_iter()
        .filter(|&d| d as f32 / 96.0 != first)
    {
        let scale = dpi as f32 / 96.0;
        dpi_changed(&desk, hwnd, dpi, rect(0, 0, 10, 10));
        let look = unsafe { toast::probe::look() }.expect("still shown");
        assert_eq!(look.scale, scale, "{dpi}");
        assert_eq!(edges(bounds(hwnd)), resting(scale), "{dpi}");
        // メニューのボタン（280〜320 DIP）も新しい倍率の所にある。
        desk.mouse(hwnd, WM_MOUSEMOVE, dip(300.0, scale), dip(24.0, scale));
        assert!(unsafe { toast::probe::look() }.unwrap().menu_hover, "{dpi}");
        desk.send(hwnd, WM_MOUSELEAVE, 0, 0);
        // 同じ倍率の知らせでは動かない。
        dpi_changed(&desk, hwnd, dpi, rect(0, 0, 10, 10));
        assert_eq!(edges(bounds(hwnd)), resting(scale));
    }
}
