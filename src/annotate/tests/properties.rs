//! 切り抜きと線の描画の性質。切り抜きは、範囲を画像に収めた部分の画素をそのまま写す。線は
//! ブラシの届かない画素を変えず、ペンは線の内側を自分の色で塗り、蛍光ペンは何度なぞっても
//! 1 回だけ重ねる。切り抜いた一部に同じだけずらした線を描くと、全体に描いた画像の同じ部分に
//! なる（書き込み中の表示と、色の取得がこれに頼っている）。

use super::*;
use crate::test_support::{
    distance_to_line,
    rng::{self, Reach, Rng},
};

const SCALES: [f32; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];
/// 画素の中心から角までの距離。中心がブラシの縁からこれより遠い画素は、ブラシと重ならない。
const HALF_DIAGONAL: f64 = std::f64::consts::FRAC_1_SQRT_2;

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
    let mut reach = Reach::new(&["empty", "inside", "clipped", "whole"]);
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
                    // 画素の四角がブラシ（線に沿って動かした円）と重なるか。
                    let reached = marks.iter().any(|mark| {
                        distance_to_line(&mark.points, center(x, y))
                            .is_some_and(|d| d <= mark.radius as f64 + HALF_DIAGONAL)
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
                    // 画素の四角全体が、線の帯（ブラシを線に沿って動かした形）の 1 画素以上
                    // 内側にある。1 画素の余裕は、ブラシを置く間隔と縁のぼかしの分。
                    if distance + HALF_DIAGONAL + 1.0 > mark.radius as f64 {
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
fn moved_marks_on_a_cut_out_part_match_that_part_of_the_whole_drawing() {
    let mut reach = Reach::new(&["compared a part", "empty part", "picked a color"]);
    // 1 段階ずれた値の数。浮動小数点の丸めで、ずらした座標の計算がわずかに変わる。
    let (mut exact, mut off_by_one) = (0usize, 0usize);
    let cases = rng::check(
        "moved_marks_on_a_cut_out_part_match_that_part_of_the_whole_drawing",
        300,
        |rng| {
            let (width, height) = (rng.range(1..=32), rng.range(1..=32));
            let pixels = image(rng, width, height);
            let marks: Vec<Mark> = (0..rng.range(1..=3))
                .map(|_| mark(rng, width, height))
                .collect();
            let mut whole = pixels.clone();
            draw(&mut whole, width, height, &marks);
            // 書き込み中の表示と同じ手順: 線を囲む範囲を切り抜き、その左上の分だけ線をずらして描く。
            let (x0, y0, x1, y1) = area(rng, width, height);
            let area = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
            if let Some((w, h, mut part)) = crop(&pixels, width, height, area) {
                let (left, top) = (area.0.max(0) as f32, area.1.max(0) as f32);
                let moved: Vec<Mark> = marks
                    .iter()
                    .map(|mark| Mark {
                        points: mark
                            .points
                            .iter()
                            .map(|&(x, y)| (x - left, y - top))
                            .collect(),
                        ..mark.clone()
                    })
                    .collect();
                draw(&mut part, w, h, &moved);
                let (_, _, expected) = crop(&whole, width, height, area).unwrap();
                for (at, (&got, &want)) in part.iter().zip(&expected).enumerate() {
                    let pixel = at / 4;
                    let (x, y) = (pixel as i32 % w, pixel as i32 / w);
                    assert!(
                        got.abs_diff(want) <= 1,
                        "{x},{y} of {area:?}: {got} for {want}, {marks:?}"
                    );
                    if got == want {
                        exact += 1;
                    } else {
                        off_by_one += 1;
                    }
                }
                reach.hit("compared a part");
            } else {
                reach.hit("empty part");
            }
            // 色の取得: 見えている画素の色は、全体に描いた画像の色と同じ。不透明度が 0 か 1 の
            // 画素は、1 段階のずれで見えるかどうかが変わるので比べない。
            let (x, y) = (rng.range(0..width), rng.range(0..height));
            let at = ((y * width + x) * 4) as usize;
            if whole[at + 3] >= 2 {
                let picked = shown_color(&pixels, width, height, &marks, (x, y))
                    .unwrap_or_else(|| panic!("{x},{y} is shown: {marks:?}"));
                let flat = [whole[at + 2], whole[at + 1], whole[at]];
                for (p, f) in picked.iter().zip(flat) {
                    assert!(
                        p.abs_diff(f) <= 1,
                        "{x},{y}: {picked:?} for {flat:?}, {marks:?}"
                    );
                }
                reach.hit("picked a color");
            }
        },
    );
    eprintln!("moved marks: {exact} values were the same and {off_by_one} were one step apart");
    reach.finish("moved_marks_on_a_cut_out_part", cases);
}
