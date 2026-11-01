//! 偽物のモニターの並びで、窓を置く規則を確かめる。期待値はすべて手で計算した値。
//!
//! | 並び | 内容 |
//! |---|---|
//! | `beside` | 主モニター A（1920x1080、100%）の左に B（1920x1080、125%、負の座標） |
//! | `stacked` | A の上に C（2560x1440、200%、負の座標）。C は A より右へ 640 はみ出す |
//! | `mixed` | B（125%）、A（100%）、右に D（2560x1440、150%） |

use super::*;

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
    RECT {
        left,
        top,
        right,
        bottom,
    }
}

fn edges(r: RECT) -> (i32, i32, i32, i32) {
    (r.left, r.top, r.right, r.bottom)
}

fn p(x: i32, y: i32) -> POINT {
    POINT { x, y }
}

/// タスクバー（`taskbar` 画素）を下に持つモニター。
fn monitor(r: RECT, dpi: u32, taskbar: i32) -> Monitor {
    Monitor {
        rect: r,
        work: RECT {
            bottom: r.bottom - taskbar,
            ..r
        },
        dpi,
    }
}

const A: RECT = RECT {
    left: 0,
    top: 0,
    right: 1920,
    bottom: 1080,
};
const B: RECT = RECT {
    left: -1920,
    top: 0,
    right: 0,
    bottom: 1080,
};
const C: RECT = RECT {
    left: 0,
    top: -1440,
    right: 2560,
    bottom: 0,
};
const D: RECT = RECT {
    left: 1920,
    top: 0,
    right: 4480,
    bottom: 1440,
};

fn beside() -> Vec<Monitor> {
    vec![monitor(A, 96, 48), monitor(B, 120, 60)]
}

fn stacked() -> Vec<Monitor> {
    vec![monitor(A, 96, 48), monitor(C, 192, 96)]
}

fn mixed() -> Vec<Monitor> {
    vec![monitor(B, 120, 60), monitor(A, 96, 48), monitor(D, 144, 72)]
}

/// 選ばれたモニターの画面全体の矩形。
fn chosen(m: Option<&Monitor>) -> Option<(i32, i32, i32, i32)> {
    m.map(|m| edges(m.rect))
}

#[test]
fn dpi_values_become_scales_and_the_message_carries_the_horizontal_dpi() {
    for (dpi, expected) in [(96, 1.0), (120, 1.25), (144, 1.5), (192, 2.0)] {
        assert_eq!(scale(dpi), expected, "{dpi}");
        // WM_DPICHANGED の wParam は上位に縦、下位に横の DPI を持つ。
        assert_eq!(
            message_scale(((dpi as usize) << 16) | dpi as usize),
            expected
        );
    }
    // 下位の横の DPI だけを読み、0 は 100% とみなす。
    assert_eq!(message_scale(0x0060_0090), 1.5);
    assert_eq!(message_scale(0), 1.0);
    assert_eq!(message_scale(0xffff_0000_0000_0078), 1.25);
}

#[test]
fn a_point_belongs_to_its_monitor_even_at_negative_coordinates() {
    let list = beside();
    // 左のモニターは負の座標。左端は含み、右端（x = 0）は主モニターのもの。
    assert_eq!(chosen(nearest(&list, p(-1920, 0))), Some(edges(B)));
    assert_eq!(chosen(nearest(&list, p(-1, 1079))), Some(edges(B)));
    assert_eq!(chosen(nearest(&list, p(0, 0))), Some(edges(A)));
    assert_eq!(chosen(nearest(&list, p(1919, 1079))), Some(edges(A)));
    // どこにもない点は、いちばん近いモニター: 左のはるか外は B、右下の外は A。
    assert_eq!(chosen(nearest(&list, p(-5000, 500))), Some(edges(B)));
    assert_eq!(chosen(nearest(&list, p(2500, 1500))), Some(edges(A)));
    // B の真下（x = -10）: 下への距離はどちらも同じで、横は B の中、A からは左へ外れる。
    assert_eq!(chosen(nearest(&list, p(-10, 1200))), Some(edges(B)));
    assert!(nearest(&[], p(0, 0)).is_none());
}

#[test]
fn a_point_beside_a_stacked_pair_goes_to_the_closer_one() {
    let list = stacked();
    // C の下の右の空き（x 1920〜2560、y 0 以上）: (2000, 500) は A の右端の列 1919 から 81、
    // C の下端の行 -1 から 501。
    assert_eq!(chosen(nearest(&list, p(2000, 500))), Some(edges(A)));
    // (2400, 30) は A から 481、C から 31。
    assert_eq!(chosen(nearest(&list, p(2400, 30))), Some(edges(C)));
    // 上のモニターの中（負の座標）。
    assert_eq!(chosen(nearest(&list, p(10, -1440))), Some(edges(C)));
    assert_eq!(chosen(nearest(&list, p(10, -1))), Some(edges(C)));
}

#[test]
fn a_window_belongs_to_the_monitor_it_covers_most() {
    let list = mixed();
    // 境目をまたぐ範囲: 左へ 100、右へ 300 なら A（100%）、左へ 300、右へ 100 なら B（125%）。
    let right_heavy = most_of(&list, rect(-100, 100, 300, 200)).unwrap();
    assert_eq!((edges(right_heavy.rect), right_heavy.dpi), (edges(A), 96));
    let left_heavy = most_of(&list, rect(-300, 100, 100, 200)).unwrap();
    assert_eq!((edges(left_heavy.rect), left_heavy.dpi), (edges(B), 120));
    // A と D（150%）の境目: 1900〜2100 は D が 180 列、A が 20 列。
    let on_d = most_of(&list, rect(1900, 0, 2100, 48)).unwrap();
    assert_eq!((edges(on_d.rect), on_d.dpi), (edges(D), 144));
    // 同じだけ重なれば一覧の先（B）。
    assert_eq!(
        chosen(most_of(&list, rect(-200, 0, 200, 10))),
        Some(edges(B))
    );
    // どれとも重ならなければいちばん近いモニター: D の下（y 1440 以上）は D、A の下は A。
    assert_eq!(
        chosen(most_of(&list, rect(3000, 1500, 3100, 1600))),
        Some(edges(D))
    );
    assert_eq!(
        chosen(most_of(&list, rect(100, 1200, 200, 1300))),
        Some(edges(A))
    );
    assert!(most_of(&[], rect(0, 0, 1, 1)).is_none());
}

#[test]
fn a_rectangle_is_on_screen_only_when_every_pixel_is_on_a_monitor() {
    let list = beside();
    // 1 台の中、隣り合う 2 台にまたがる、空の矩形。
    assert!(covered(&list, rect(100, 100, 432, 148)));
    assert!(covered(&list, rect(-100, 12, 232, 60)));
    assert!(covered(&list, rect(5, 5, 5, 5)));
    // 右端、下端、左端の外へ 1 画素でも出れば外。
    assert!(!covered(&list, rect(1600, 12, 1921, 60)));
    assert!(!covered(&list, rect(100, 1040, 432, 1081)));
    assert!(!covered(&list, rect(-1921, 12, -1589, 60)));
    // 上下の並びで、C だけが右へはみ出す所: 上半分は C の上、下半分は A の右の外。
    let list = stacked();
    assert!(covered(&list, rect(1900, -10, 1920, 10)));
    assert!(!covered(&list, rect(1900, -10, 1950, 10)));
    // 隙間を空けて並ぶ 2 台をまたぐ矩形は、四隅がどちらも画面の上でも、間が外。
    let gapped = [
        monitor(A, 96, 48),
        monitor(rect(2000, 0, 3920, 1080), 96, 48),
    ];
    assert!(!covered(&gapped, rect(1900, 100, 2100, 200)));
    assert!(!covered(&[], rect(0, 0, 1, 1)));
}

#[test]
fn a_window_pushed_off_the_screen_comes_back_onto_the_nearest_monitor() {
    // 画面の上なら動かさない（2 台にまたがっていても）。
    let list = beside();
    for r in [rect(300, 80, 632, 128), rect(-100, 12, 232, 60)] {
        assert_eq!(edges(keep_on_screen(&list, r)), edges(r));
    }
    // 主モニターの解像度を 1280x720 に下げた: 1500 の所のツールバーは右端の内側へ。
    let lowered = [monitor(rect(0, 0, 1280, 720), 96, 48)];
    assert_eq!(
        edges(keep_on_screen(&lowered, rect(1500, 12, 1832, 60))),
        (948, 12, 1280, 60)
    );
    // 左のモニター（負の座標）を外した: 残った主モニターの左端へ。
    let removed = [monitor(A, 96, 48)];
    assert_eq!(
        edges(keep_on_screen(&removed, rect(-1000, 12, -668, 60))),
        (0, 12, 332, 60)
    );
    // 上下の並びで右へはみ出した所: いちばん広く重なる C の中へ（下端を C の下端にそろえる）。
    assert_eq!(
        edges(keep_on_screen(&stacked(), rect(2200, -30, 2532, 18))),
        (2200, -48, 2532, 0)
    );
    // モニターより大きければ、左端と上端をそろえる。
    assert_eq!(
        edges(fit(rect(-50, -50, 2950, 1950), A)),
        (0, 0, 3000, 2000)
    );
    // モニターがなければ、そのまま。
    assert_eq!(edges(keep_on_screen(&[], rect(1, 2, 3, 4))), (1, 2, 3, 4));
}

#[test]
fn the_top_center_follows_each_monitor_and_its_scale() {
    // 主モニター（100%）: 幅 332 なら (1920 - 332) / 2 = 794、上から 12。
    assert_eq!(edges(top_center(A, 332, 48, 1.0)), (794, 12, 1126, 60));
    // 左のモニター（125%）: 幅 415、(-1920 - 415) / 2 = -1167.5 は 0 の側へ切り捨てて -1167、上から 15。
    assert_eq!(edges(top_center(B, 415, 60, 1.25)), (-1167, 15, -752, 75));
    // 上のモニター（200%）: 幅 664、(2560 - 664) / 2 = 948、上端 -1440 から 24。
    assert_eq!(
        edges(top_center(C, 664, 96, 2.0)),
        (948, -1416, 1612, -1320)
    );
    // 右のモニター（150%）: 幅 498、(1920 + 4480 - 498) / 2 = 2951、上から 18。
    assert_eq!(edges(top_center(D, 498, 72, 1.5)), (2951, 18, 3449, 90));
}

#[test]
fn only_changes_under_an_area_move_what_the_area_shows() {
    let before = beside();
    let on_a = rect(100, 100, 400, 300);
    let on_b = rect(-800, 100, -500, 300);
    let across = rect(-100, 100, 100, 300);
    // 左のモニターを外した: A の上の範囲はそのまま、B の上と境目をまたぐ範囲は変わった。
    let removed = [monitor(A, 96, 48)];
    assert!(!moved_under(&before, &removed, on_a));
    assert!(moved_under(&before, &removed, on_b));
    assert!(moved_under(&before, &removed, across));
    // 主モニターの解像度を上げた: B の上の範囲はそのまま。
    let raised = [monitor(rect(0, 0, 2560, 1440), 96, 48), monitor(B, 120, 60)];
    assert!(!moved_under(&before, &raised, on_b));
    assert!(moved_under(&before, &raised, on_a));
    // 左のモニターを右へ動かした: B の座標の範囲は、もうどのモニターの上にもない。
    let moved = [
        monitor(A, 96, 48),
        monitor(rect(1920, 0, 3840, 1080), 120, 60),
    ];
    assert!(!moved_under(&before, &moved, on_a));
    assert!(moved_under(&before, &moved, on_b));
    // 拡大率と作業領域だけの変化、並べる順番の違いは、並びを変えない。
    let rescaled = [monitor(B, 144, 72), monitor(A, 192, 96)];
    assert!(!moved_under(&before, &rescaled, across));
    assert!(same_layout(&before, &rescaled));
    assert!(!same_layout(&before, &removed));
    assert!(!same_layout(&before, &raised));
    assert!(!same_layout(&before, &moved));
    assert!(same_layout(&[], &[]));
}
