//! 切り抜きと線の描画の性質。切り抜きは、範囲を画像に収めた部分の画素をそのまま写し、画素の数が
//! 大きさと合わなければ何も切り抜かない。線の覆いは画素の中心から線までの距離で決まり、縁は
//! 半径の内外 0.5 画素ずつの幅で消える（点を並べて押したときのような波がない）。ブラシの届かない
//! 画素は変えず、ペンは線の内側を自分の色で塗り、蛍光ペンは何度なぞっても 1 回だけ重ねる。
//! 切り抜いた一部にその位置で線を描くと、全体に描いた画像の同じ部分とぴったり同じになる
//! （書き込み中の表示と、色の取得がこれに頼っている）。

use super::*;
use crate::test_support::{
    distance_to_line,
    rng::{self, Reach, Rng},
};

const SCALES: [f32; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];
/// 縁のぼかしの幅の半分（画素）。中心が線から `半径 - EDGE` 以内の画素は全部覆われ、
/// `半径 + EDGE` より遠い画素は少しも覆われない。
const EDGE: f64 = 0.5;
/// 線までの距離を、描く側とは別の式（外積）で求めたときの丸めの差の上限（画素）。どちらも f64 で
/// 差は 1e-12 ほど。点を並べて押したときの縁の波（半径の 3%、いちばん細いペンでも 0.04 画素）
/// よりずっと小さい。
const ROUNDING: f64 = 1e-9;

/// 画像: 不透明、透明、半透明の画素が混ざる。
fn image(rng: &mut Rng, width: i32, height: i32) -> Vec<u8> {
    let mut pixels = rng.bytes((width * height * 4) as usize);
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel[3] = match rng.below(4) {
            0 | 1 => 255,
            2 => 0,
            _ => pixel[3],
        };
    }
    pixels
}

/// 線: 画像の少し外まで届く点を 1〜6 個。半分はマウスから来る画素の中心。行って戻る線もある。
fn mark(rng: &mut Rng, width: i32, height: i32) -> Mark {
    let highlight = rng.chance(0.4);
    let radius = if rng.chance(0.8) {
        let radii = if highlight {
            HIGHLIGHTER_RADII
        } else {
            PEN_RADII
        };
        *rng.pick(&radii) * *rng.pick(&SCALES)
    } else {
        rng.float(0.2, 12.0)
    };
    let centers = rng.chance(0.5);
    let mut points: Vec<(f32, f32)> = (0..rng.range(1..=6))
        .map(|_| {
            let x = rng.float(-8.0, width as f32 + 8.0);
            let y = rng.float(-8.0, height as f32 + 8.0);
            if centers {
                (x.floor() + 0.5, y.floor() + 0.5)
            } else {
                (x, y)
            }
        })
        .collect();
    if rng.chance(0.2) {
        let back: Vec<(f32, f32)> = points.iter().rev().skip(1).copied().collect();
        points.extend(back);
    }
    Mark {
        highlight,
        color: *rng.pick(&COLORS),
        radius,
        points,
    }
}

fn center(x: i32, y: i32) -> (f32, f32) {
    (x as f32 + 0.5, y as f32 + 0.5)
}

/// 範囲 `(x0, y0, x1, y1)`: 画像の中、画像全体かそれより広い範囲、画像のまわりの無作為な範囲
/// （向きが逆のもの、空のもの、画像からはみ出すものを含む）。
fn area(rng: &mut Rng, width: i32, height: i32) -> (i32, i32, i32, i32) {
    match rng.below(10) {
        0 => {
            let mut out = || rng.range(0..=3);
            (-out(), -out(), width + out(), height + out())
        }
        1..=4 => {
            let (x0, y0) = (rng.range(0..width), rng.range(0..height));
            (
                x0,
                y0,
                rng.range(x0 + 1..=width),
                rng.range(y0 + 1..=height),
            )
        }
        _ => {
            let mut edge = |size: i32| rng.range(-6..=size + 6);
            (edge(width), edge(height), edge(width), edge(height))
        }
    }
}

#[test]
fn cropping_copies_exactly_the_area_clipped_to_the_image() {
    let mut reach = Reach::new(&["empty", "inside", "clipped", "whole", "wrong size"]);
    let cases = rng::check(
        "cropping_copies_exactly_the_area_clipped_to_the_image",
        500,
        |rng| {
            let (width, height) = (rng.range(1..=24), rng.range(1..=24));
            let pixels = image(rng, width, height);
            let area = area(rng, width, height);
            let (x0, y0, x1, y1) = area;
            let (left, top) = (x0.clamp(0, width), y0.clamp(0, height));
            let (right, bottom) = (x1.clamp(0, width), y1.clamp(0, height));
            let what = format!("{area:?} of {width}x{height}");
            // 画素が大きさより短いか長い、または大きさが負なら、どの範囲でも切り抜かない
            // （足りない画素を読みに行って panic しない）。
            let mut wrong = pixels.clone();
            let (w, h) = match rng.below(3) {
                0 => {
                    wrong.truncate(rng.below(wrong.len()));
                    (width, height)
                }
                1 => {
                    let extra = rng.range(1..=8);
                    wrong.extend(rng.bytes(extra));
                    (width, height)
                }
                _ => *rng.pick(&[(-width, height), (width, -height)]),
            };
            assert!(
                crop(&wrong, w, h, area).is_none(),
                "{} bytes as {w}x{h}: {what}",
                wrong.len()
            );
            reach.hit("wrong size");
            let result = crop(&pixels, width, height, area);
            if right <= left || bottom <= top {
                assert!(result.is_none(), "{what} is empty");
                reach.hit("empty");
                return;
            }
            let (w, h, part) = result.unwrap_or_else(|| panic!("{what} has pixels"));
            assert_eq!((w, h), (right - left, bottom - top), "{what}");
            assert_eq!(part.len(), (w * h * 4) as usize, "{what}");
            for y in 0..h {
                for x in 0..w {
                    let from = (((top + y) * width + left + x) * 4) as usize;
                    let to = ((y * w + x) * 4) as usize;
                    assert_eq!(
                        part[to..to + 4],
                        pixels[from..from + 4],
                        "{x},{y} of {what}"
                    );
                }
            }
            reach.hit(if (w, h) == (width, height) {
                assert_eq!(part, pixels, "the whole image is copied unchanged");
                "whole"
            } else if (left, top, right, bottom) == area {
                "inside"
            } else {
                "clipped"
            });
        },
    );
    reach.finish("cropping_copies_exactly_the_area", cases);
}

#[test]
fn strokes_leave_every_pixel_the_brush_does_not_reach_unchanged() {
    let mut reach = Reach::new(&["some pixels kept", "some pixels painted"]);
    let cases = rng::check(
        "strokes_leave_every_pixel_the_brush_does_not_reach_unchanged",
        300,
        |rng| {
            let (width, height) = (rng.range(1..=32), rng.range(1..=32));
            let pixels = image(rng, width, height);
            let marks: Vec<Mark> = (0..rng.range(1..=3))
                .map(|_| mark(rng, width, height))
                .collect();
            let mut drawn = pixels.clone();
            draw(&mut drawn, width, height, &marks);
            let (mut kept, mut painted) = (false, false);
            for y in 0..height {
                for x in 0..width {
                    let at = ((y * width + x) * 4) as usize;
                    // 画素の中心が、ブラシ（線に沿って動かした円）の縁のぼかしより内側にあるか。
                    let reached = marks.iter().any(|mark| {
                        distance_to_line(&mark.points, center(x, y))
                            .is_some_and(|d| d < mark.radius as f64 + EDGE + ROUNDING)
                    });
                    if !reached {
                        assert_eq!(drawn[at..at + 4], pixels[at..at + 4], "{x},{y}: {marks:?}");
                        kept = true;
                    } else if drawn[at..at + 4] != pixels[at..at + 4] {
                        painted = true;
                    }
                }
            }
            if kept {
                reach.hit("some pixels kept");
            }
            if painted {
                reach.hit("some pixels painted");
            }
        },
    );
    reach.finish("strokes_leave_every_pixel_unchanged", cases);
}

#[test]
fn well_inside_a_line_the_pen_paints_its_color_and_the_highlighter_tints_once() {
    let mut reach = Reach::new(&["pen", "highlighter over opaque", "highlighter over clear"]);
    let cases = rng::check(
        "well_inside_a_line_the_pen_paints_its_color_and_the_highlighter_tints_once",
        300,
        |rng| {
            let (width, height) = (rng.range(4..=40), rng.range(4..=40));
            let pixels = image(rng, width, height);
            let mark = mark(rng, width, height);
            let mut drawn = pixels.clone();
            draw(&mut drawn, width, height, std::slice::from_ref(&mark));
            let [r, g, b] = mark.color;
            let near = |value: u8, expected: f64| (value as f64 - expected).abs() <= 1.0;
            for y in 0..height {
                for x in 0..width {
                    let Some(distance) = distance_to_line(&mark.points, center(x, y)) else {
                        continue;
                    };
                    // 画素の中心が、線の帯（ブラシを線に沿って動かした形）の縁から、縁のぼかしの
                    // 半分より内側にある。縁が波打つと、へこんだ所でここの覆いが足りなくなる。
                    if distance + EDGE + ROUNDING > mark.radius as f64 {
                        continue;
                    }
                    let at = ((y * width + x) * 4) as usize;
                    let (before, after) = (&pixels[at..at + 4], &drawn[at..at + 4]);
                    let what = format!("{x},{y} from {before:?}: {mark:?}");
                    if !mark.highlight {
                        assert_eq!(after, [b, g, r, 255], "the pen covers {what}");
                        reach.hit("pen");
                        continue;
                    }
                    // 蛍光ペンは 45% で 1 回だけ重なる。不透明な画素では色を混ぜ、透明な画素
                    // ではその色が 45% の不透明度で乗る。
                    match before[3] {
                        255 => {
                            for (channel, color) in [b, g, r].into_iter().enumerate() {
                                let mixed = color as f64 * 0.45 + before[channel] as f64 * 0.55;
                                assert!(near(after[channel], mixed), "tinted once {what}");
                            }
                            assert_eq!(after[3], 255, "{what}");
                            reach.hit("highlighter over opaque");
                        }
                        0 => {
                            assert_eq!(after[..3], [b, g, r], "{what}");
                            assert!(near(after[3], 0.45 * 255.0), "{what}");
                            reach.hit("highlighter over clear");
                        }
                        _ => {}
                    }
                }
            }
        },
    );
    reach.finish("well_inside_a_line", cases);
}

#[test]
fn the_pen_fades_out_over_one_pixel_by_the_distance_to_the_line() {
    let mut reach = Reach::new(&["fully covered", "on the edge", "untouched"]);
    let cases = rng::check(
        "the_pen_fades_out_over_one_pixel_by_the_distance_to_the_line",
        300,
        |rng| {
            let (width, height) = (rng.range(1..=40), rng.range(1..=40));
            let mark = Mark {
                highlight: false,
                ..mark(rng, width, height)
            };
            // 透明な画像にペン（不透明度 1）で描くと、画素の不透明度がそのまま覆いになる。
            let mut drawn = vec![0u8; (width * height * 4) as usize];
            draw(&mut drawn, width, height, std::slice::from_ref(&mark));
            let [r, g, b] = mark.color;
            // 覆いは、中心が線から半径 - EDGE のところで 1、半径 + EDGE のところで 0 になる直線で、
            // 255 段階に切り捨てる。どの向きの線のどの位置でも同じなので、縁は波打たない。
            let ramp = |d: f64| (mark.radius as f64 + EDGE - d).clamp(0.0, 1.0) * 255.0;
            for y in 0..height {
                for x in 0..width {
                    let distance = distance_to_line(&mark.points, center(x, y)).unwrap();
                    let (most, least) = (ramp(distance - ROUNDING), ramp(distance + ROUNDING));
                    let at = ((y * width + x) * 4) as usize;
                    let alpha = drawn[at + 3];
                    let what = format!("{x},{y} at {distance} from {mark:?}");
                    assert!(
                        alpha as f64 <= most && alpha as f64 > least - 1.0,
                        "a coverage of {alpha} for {least}..{most}: {what}"
                    );
                    if alpha > 0 {
                        assert_eq!(drawn[at..at + 3], [b, g, r], "{what}");
                    }
                    reach.hit(match alpha {
                        255 => "fully covered",
                        0 => "untouched",
                        _ => "on the edge",
                    });
                }
            }
        },
    );
    reach.finish("the_pen_fades_out", cases);
}

#[test]
fn marks_drawn_on_a_cut_out_part_match_that_part_of_the_whole_drawing() {
    let mut reach = Reach::new(&[
        "compared a part",
        "empty part",
        "picked a color",
        "nothing to pick",
    ]);
    // 丸めで 1 段階ずれる値はまれで、直す前の手順（線をずらしてから描く）でも、最初にずれが出る
    // のは 7〜1,264 ケース目だった（シード 11 通り）。どれでも見つかるように多めに回す。
    let cases = rng::check(
        "marks_drawn_on_a_cut_out_part_match_that_part_of_the_whole_drawing",
        2000,
        |rng| {
            let (width, height) = (rng.range(1..=32), rng.range(1..=32));
            let pixels = image(rng, width, height);
            let marks: Vec<Mark> = (0..rng.range(1..=3))
                .map(|_| mark(rng, width, height))
                .collect();
            let mut whole = pixels.clone();
            draw(&mut whole, width, height, &marks);
            // 書き込み中の表示と同じ手順: 線を囲む範囲を切り抜き、線は画像の座標のまま、
            // 切り抜いた位置に描く。
            let (x0, y0, x1, y1) = area(rng, width, height);
            let area = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
            if let Some((w, h, mut part)) = crop(&pixels, width, height, area) {
                draw_part(&mut part, (area.0.max(0), area.1.max(0)), w, h, &marks);
                let (_, _, expected) = crop(&whole, width, height, area).unwrap();
                if let Some(at) = (0..part.len()).find(|&at| part[at] != expected[at]) {
                    let (x, y) = ((at / 4) as i32 % w, (at / 4) as i32 / w);
                    panic!(
                        "{x},{y} of {area:?}: {} for {}, {marks:?}",
                        part[at], expected[at]
                    );
                }
                reach.hit("compared a part");
            } else {
                reach.hit("empty part");
            }
            // 色の取得（1 画素の部分に描く）: どの画素でも、全体に描いた画像の色と同じで、
            // 透明な画素には色がない。
            for y in 0..height {
                for x in 0..width {
                    let at = ((y * width + x) * 4) as usize;
                    let flat =
                        (whole[at + 3] != 0).then(|| [whole[at + 2], whole[at + 1], whole[at]]);
                    assert_eq!(
                        shown_color(&pixels, width, height, &marks, (x, y)),
                        flat,
                        "{x},{y}: {marks:?}"
                    );
                    reach.hit(if flat.is_some() {
                        "picked a color"
                    } else {
                        "nothing to pick"
                    });
                }
            }
        },
    );
    reach.finish("marks_drawn_on_a_cut_out_part", cases);
}
