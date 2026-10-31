//! 消しゴムが線に届くかの判定（`touches`）と、バーの置き場所（`place`）の性質。消しゴムは、
//! 前の点から今の点までの道筋（線分）で判定する。押しただけのときは道筋が 1 点になる。

use super::*;
use crate::test_support::{
    distance_to_line,
    rng::{self, Reach, Rng},
};

/// 判定の境目からこれより近い点は、浮動小数点の丸めでどちらにも転びうるので比べない。
const BORDER: f64 = 1e-2;
/// 道筋を歩く回数の上限。境目に沿って長く進む道筋は、どちらとも言わずに飛ばす。
const WALK: usize = 100_000;

/// 線: 点は 0〜6 個。同じ点が続くこと（長さ 0 の線分）もある。半分はマウスから来る画素の中心。
fn mark(rng: &mut Rng) -> Mark {
    let count = *rng.pick(&[0, 1, 1, 2, 2, 3, 4, 6]);
    let mut at = (rng.float(-50.0, 250.0), rng.float(-50.0, 250.0));
    let mut points = Vec::new();
    for _ in 0..count {
        points.push(at);
        if rng.below(4) > 0 {
            at = (at.0 + rng.float(-40.0, 40.0), at.1 + rng.float(-40.0, 40.0));
        }
    }
    if rng.chance(0.5) {
        for point in &mut points {
            *point = (point.0.floor() + 0.5, point.1.floor() + 0.5);
        }
    }
    let radius = if rng.chance(0.8) {
        let radii = if rng.chance(0.5) {
            annotate::PEN_RADII
        } else {
            annotate::HIGHLIGHTER_RADII
        };
        *rng.pick(&radii) * *rng.pick(&SCALES)
    } else {
        rng.float(0.0, 20.0)
    };
    line(&points, radius)
}

/// 調べる点: たいていは線のそばで、境目（太さ + 届く距離）の内と外にまたがる。
fn probe(rng: &mut Rng, mark: &Mark, reach: f32) -> (f32, f32) {
    if mark.points.is_empty() || rng.chance(0.2) {
        return (rng.float(-80.0, 300.0), rng.float(-80.0, 300.0));
    }
    let at = rng.below(mark.points.len());
    let a = mark.points[at];
    let b = mark.points[(at + 1).min(mark.points.len() - 1)];
    let t = rng.float(-0.2, 1.2);
    let angle = rng.float(0.0, std::f32::consts::TAU);
    let offset = (mark.radius + reach) * rng.float(0.0, 2.0);
    (
        a.0 + (b.0 - a.0) * t + offset * angle.cos(),
        a.1 + (b.1 - a.1) * t + offset * angle.sin(),
    )
}

/// 消しゴムの道筋（始まり, 終わり）。4 割は押しただけの 1 点。残りは線のそばの点を通って横切る
/// 線分で、両端が線から離れていても間で線に触れる（速くなぞって飛び越える）ことが多い。
fn path(rng: &mut Rng, mark: &Mark, reach: f32) -> ((f32, f32), (f32, f32)) {
    let p = probe(rng, mark, reach);
    if rng.chance(0.4) {
        return (p, p);
    }
    let angle = rng.float(0.0, std::f32::consts::TAU);
    let half = (mark.radius + reach) * rng.float(0.5, 3.0) + rng.float(0.0, 10.0);
    let (dx, dy) = (half * angle.cos(), half * angle.sin());
    let back = rng.float(0.5, 1.5);
    ((p.0 + dx, p.1 + dy), (p.0 - dx * back, p.1 - dy * back))
}

/// 別の方法で求めた答え: 道筋の上を、点から線までの距離（`distance_to_line`）だけを使って歩く。
/// 線までの距離は、道筋に沿って進んだ長さより速くは変わらない。そのため、今の距離が境目
/// （太さ + 届く距離）より `BORDER` 以上遠ければ、その差から `BORDER` の半分を引いた分だけ
/// 進んでも、その間で境目の内側には入らない。境目より `BORDER` 以上内側の点に来たら触れる。
/// 境目から `BORDER` より近い点を通り、その先でも内側に入らなければ、どちらとも言わず `None`。
fn expected(mark: &Mark, from: (f32, f32), to: (f32, f32), reach: f32) -> Option<bool> {
    let limit = (mark.radius + reach) as f64;
    let (dx, dy) = ((to.0 - from.0) as f64, (to.1 - from.1) as f64);
    let length = dx.hypot(dy);
    let (mut walked, mut near) = (0.0, false);
    for _ in 0..WALK {
        let t = if length > 0.0 { walked / length } else { 0.0 };
        let at = (
            (from.0 as f64 + dx * t) as f32,
            (from.1 as f64 + dy * t) as f32,
        );
        let Some(distance) = distance_to_line(&mark.points, at) else {
            return Some(false);
        };
        if distance <= limit - BORDER {
            return Some(true);
        }
        near |= distance < limit + BORDER;
        if walked >= length {
            return (!near).then_some(false);
        }
        walked = (walked + (distance - limit - BORDER / 2.0).max(BORDER / 2.0)).min(length);
    }
    None
}

/// 比べずに飛ばした、境目に近すぎるケースは少ない（多ければ、何も確かめていない）。
fn assert_few_borderline(borderline: usize, cases: usize) {
    eprintln!("{borderline} of {cases} cases were too close to the border to compare");
    assert!(
        borderline * 20 <= cases,
        "{borderline} of {cases} cases were skipped"
    );
}

#[test]
fn touches_agrees_with_the_distance_to_the_line_found_another_way() {
    let mut reach = Reach::new(&[
        "touches",
        "misses",
        "no points",
        "one point",
        "pressed only",
        "jumped over",
    ]);
    let mut borderline = 0;
    let cases = rng::check(
        "touches_agrees_with_the_distance_to_the_line_found_another_way",
        2000,
        |rng| {
            let mark = mark(rng);
            let eraser = rng.float(0.0, 20.0);
            let (from, to) = path(rng, &mark, eraser);
            match mark.points.len() {
                0 => reach.hit("no points"),
                1 => reach.hit("one point"),
                _ => {}
            }
            if from == to {
                reach.hit("pressed only");
            }
            match expected(&mark, from, to, eraser) {
                None => borderline += 1,
                Some(touch) => {
                    let what = format!("{mark:?} {from:?}-{to:?} {eraser}");
                    assert_eq!(touches(&mark, from, to, eraser), touch, "{what}");
                    reach.hit(if touch { "touches" } else { "misses" });
                    // 両端は届かないのに、間の道筋が線に触れる（速くなぞって飛び越える）。
                    let ends = [from, to].map(|p| expected(&mark, p, p, eraser));
                    if touch && ends == [Some(false), Some(false)] {
                        reach.hit("jumped over");
                    }
                }
            }
        },
    );
    reach.finish("touches_agrees_with_the_distance", cases);
    assert_few_borderline(borderline, cases);
}

#[test]
fn touches_ignores_where_the_line_is_and_which_way_either_was_drawn() {
    let mut reach = Reach::new(&["moved", "reversed", "erased backwards"]);
    let mut borderline = 0;
    let cases = rng::check(
        "touches_ignores_where_the_line_is_and_which_way_either_was_drawn",
        2000,
        |rng| {
            let mark = mark(rng);
            let eraser = rng.float(0.0, 20.0);
            let (from, to) = path(rng, &mark, eraser);
            // 境目のそばでは、どちらの答えも正しいので比べない。
            if expected(&mark, from, to, eraser).is_none() {
                borderline += 1;
                return;
            }
            let what = format!("{mark:?} {from:?}-{to:?} {eraser}");
            let touch = touches(&mark, from, to, eraser);
            let (dx, dy) = (
                rng.range(-1000..=1000) as f32,
                rng.range(-1000..=1000) as f32,
            );
            let moved = Mark {
                points: mark.points.iter().map(|&(x, y)| (x + dx, y + dy)).collect(),
                ..mark.clone()
            };
            let shift = |(x, y): (f32, f32)| (x + dx, y + dy);
            assert_eq!(
                touches(&moved, shift(from), shift(to), eraser),
                touch,
                "moved by {dx},{dy}: {what}"
            );
            reach.hit("moved");
            let reversed = Mark {
                points: mark.points.iter().rev().copied().collect(),
                ..mark.clone()
            };
            assert_eq!(
                touches(&reversed, from, to, eraser),
                touch,
                "reversed: {what}"
            );
            reach.hit("reversed");
            assert_eq!(touches(&mark, to, from, eraser), touch, "backwards: {what}");
            reach.hit("erased backwards");
        },
    );
    reach.finish("touches_ignores_where_the_line_is", cases);
    assert_few_borderline(borderline, cases);
}

#[test]
fn a_longer_reach_a_thicker_line_or_a_longer_path_never_misses_what_a_shorter_one_touches() {
    let mut reach = Reach::new(&[
        "touched both",
        "touched only the longer",
        "touched neither",
        "only the path touched",
    ]);
    let cases = rng::check(
        "a_longer_reach_a_thicker_line_or_a_longer_path_never_misses_what_a_shorter_one_touches",
        2000,
        |rng| {
            let mark = mark(rng);
            let short = rng.float(0.0, 20.0);
            let long = short + rng.float(0.0, 10.0);
            let (from, to) = path(rng, &mark, short);
            let what = format!("{mark:?} {from:?}-{to:?}");
            let (near, far) = (
                touches(&mark, from, to, short),
                touches(&mark, from, to, long),
            );
            assert!(!near || far, "{short} touches but {long} does not: {what}");
            let thicker = Mark {
                radius: mark.radius + (long - short),
                ..mark.clone()
            };
            assert!(
                !near || touches(&thicker, from, to, short),
                "a thicker line is missed: {what}"
            );
            // 道筋はその両端を含むので、どちらかの端で触れれば道筋でも触れる。
            let ends = [from, to].map(|p| touches(&mark, p, p, short));
            assert!(
                !ends.contains(&true) || near,
                "an end touches but the path does not: {what}"
            );
            reach.hit(match (near, far) {
                (true, _) => "touched both",
                (false, true) => "touched only the longer",
                (false, false) => "touched neither",
            });
            if near && ends == [false, false] {
                reach.hit("only the path touched");
            }
        },
    );
    reach.finish("a_longer_reach_never_misses", cases);
}

/// バーの大きさと、範囲やモニターの端との間隔（DIP）。
const BAR: (f32, f32, f32) = (432.0, 48.0, 12.0);

/// 失敗の表示のための、左、上、右、下。
fn edges(r: RECT) -> String {
    format!("({}, {}, {}, {})", r.left, r.top, r.right, r.bottom)
}

fn bar_size(scale: f32) -> (i32, i32, i32) {
    let round = |dip: f32| (dip * scale).round() as i32;
    (round(BAR.0), round(BAR.1), round(BAR.2))
}

/// モニター（作業領域）。ときどき、バーがちょうど入るかどうかの大きさにする。
fn monitor(rng: &mut Rng, scale: f32) -> RECT {
    let (width, height, gap) = bar_size(scale);
    let (left, top) = (rng.range(-3000..=3000), rng.range(-2000..=2000));
    let (w, h) = if rng.chance(0.15) {
        (
            width + 2 * gap + rng.range(-3..=3),
            height + 2 * gap + rng.range(-3..=3),
        )
    } else {
        (rng.range(width..=4000), rng.range(height * 5..=2400))
    };
    rect(left, top, left + w, top + h)
}

/// 切り取った範囲。下端（バーを置く基準の点）はモニターの上下の中にある。下にも上にも
/// 入らない範囲が十分に出るよう、上端と下端をモニターの端に寄せることがある。ときどき、
/// バーがちょうど下や上に入るかどうかの境目と、中央に置いたバーがちょうど端の間隔に
/// 届くかどうかの境目に置く。
fn area_on(rng: &mut Rng, monitor: RECT, scale: f32) -> RECT {
    let (width, height, gap) = bar_size(scale);
    let room = 2 * gap + height + 30;
    let bottom = match rng.below(20) {
        0..=2 => (monitor.bottom - 2 * gap - height + rng.range(-1..=1)).max(monitor.top),
        3..=10 => rng.range((monitor.bottom - room).max(monitor.top)..=monitor.bottom),
        _ => rng.range(monitor.top..=monitor.bottom),
    };
    let top = match rng.below(20) {
        0..=2 => monitor.top + 2 * gap + height + rng.range(-1..=1),
        3..=10 => rng.range(monitor.top - 50..=(monitor.top + room).min(bottom)),
        _ => rng.range(monitor.top - 50..=bottom),
    }
    .min(bottom);
    let center = match rng.below(20) {
        0 | 1 => monitor.left + gap + width / 2 + rng.range(-1..=1),
        2 | 3 => monitor.right - gap - width / 2 + rng.range(-1..=1),
        _ => rng.range(monitor.left - 300..=monitor.right + 300),
    };
    let half = rng.range(0..=1200);
    rect(center - half, top, center + half + rng.range(0..=1), bottom)
}

#[test]
fn the_bar_stays_on_its_monitor_whenever_it_fits() {
    let mut reach = Reach::new(&["fits", "too small"]);
    let cases = rng::check(
        "the_bar_stays_on_its_monitor_whenever_it_fits",
        2000,
        |rng| {
            let scale = *rng.pick(&SCALES);
            let monitor = monitor(rng, scale);
            let area = area_on(rng, monitor, scale);
            let (width, height, gap) = bar_size(scale);
            let fits = monitor.right - monitor.left >= width + 2 * gap
                && monitor.bottom - monitor.top >= height + 2 * gap;
            if !fits {
                reach.hit("too small");
                return;
            }
            reach.hit("fits");
            let at = place(area, monitor, scale);
            assert!(
                at.x >= monitor.left
                    && at.y >= monitor.top
                    && at.x + width <= monitor.right
                    && at.y + height <= monitor.bottom,
                "{},{} {width}x{height} leaves {} for {} at {scale}",
                at.x,
                at.y,
                edges(monitor),
                edges(area)
            );
        },
    );
    reach.finish("the_bar_stays_on_its_monitor", cases);
}

#[test]
fn the_bar_goes_under_the_area_then_above_then_over_it_and_stays_centered() {
    let mut reach = Reach::new(&["below", "above", "over", "centered", "pushed in"]);
    let cases = rng::check(
        "the_bar_goes_under_the_area_then_above_then_over_it_and_stays_centered",
        2000,
        |rng| {
            let scale = *rng.pick(&SCALES);
            let monitor = monitor(rng, scale);
            let area = area_on(rng, monitor, scale);
            let (width, height, gap) = bar_size(scale);
            // 普通の大きさのモニターだけを見る（小さすぎると、どの置き方も範囲に重ならない）。
            if monitor.right - monitor.left < width + 2 * gap
                || monitor.bottom - monitor.top < 4 * gap + 2 * height
            {
                return;
            }
            let at = place(area, monitor, scale);
            let what = format!(
                "{},{} for {} on {} at {scale}",
                at.x,
                at.y,
                edges(area),
                edges(monitor)
            );
            if area.bottom + gap + height <= monitor.bottom - gap {
                assert_eq!(at.y, area.bottom + gap, "below: {what}");
                reach.hit("below");
            } else if area.top - gap - height >= monitor.top + gap {
                assert_eq!(at.y + height, area.top - gap, "above: {what}");
                reach.hit("above");
            } else {
                // 下にも上にも入らなければ、範囲の中の下端に重ねる。範囲がバーより低くても重なる。
                assert!(
                    at.y < area.bottom && at.y + height > area.top,
                    "over: {what}"
                );
                // バーが入る高さの範囲（モニターの中の部分）なら、バーは範囲からはみ出さない。
                let top = area.top.max(monitor.top);
                if area.bottom - top >= height + 2 * gap {
                    assert!(
                        at.y >= top && at.y + height <= area.bottom,
                        "inside: {what}"
                    );
                }
                reach.hit("over");
            }
            // 横は範囲の中央。モニターの端（間隔の内側）からはみ出すときだけ、内側へ寄せる。
            let centered = (area.left + area.right) as f32 / 2.0 - width as f32 / 2.0;
            let (lowest, highest) = (monitor.left + gap, monitor.right - gap - width);
            if centered >= lowest as f32 && centered <= highest as f32 {
                assert!((at.x as f32 - centered).abs() <= 1.0, "centered: {what}");
                reach.hit("centered");
            } else {
                let side = if centered < lowest as f32 {
                    lowest
                } else {
                    highest
                };
                assert_eq!(at.x, side, "pushed in: {what}");
                reach.hit("pushed in");
            }
        },
    );
    reach.finish("the_bar_goes_under_the_area", cases);
}
