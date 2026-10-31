//! 差分テスト: 同じ画像を PNG と BMP で保存し、読み戻した画素を比べる。
//!
//! README は「自由形式の範囲外は、PNG ファイルでは透明になり、それ以外（BMP を含む）では白に
//! なる」と言う。BMP には透明がないので、BMP は PNG を白い背景で見たときと同じ色になるはず。
//! 不透明な画素はそのまま、透明な画素は白、半透明の画素（自由形式の外側に描いた蛍光ペンや、
//! ペンの縁）は白の上に重ねた色。PNG は透明も含めて、渡した画素をそのまま持つ。

use super::*;
use crate::annotate::{self, Mark};
use crate::test_support::rng::{self, Reach, Rng};

/// 白の上に重ねた色（手計算の式）。`c * a + 255 * (1 - a)` を四捨五入する。255 は奇数なので、
/// ちょうど半分になることはない。
fn on_white(pixel: [u8; 4]) -> [u8; 4] {
    let alpha = pixel[3] as f64 / 255.0;
    let mix = |c: u8| (c as f64 * alpha + 255.0 * (1.0 - alpha)).round() as u8;
    [mix(pixel[0]), mix(pixel[1]), mix(pixel[2]), 255]
}

/// 不透明、白の透明（自由形式の外側）、色の付いた透明、半透明の画素が混ざる画像。
fn mixed(rng: &mut Rng, reach: &mut Reach) -> (i32, i32, Vec<u8>) {
    let (width, height) = (rng.range(1..=24), rng.range(1..=24));
    let mut pixels = rng.bytes((width * height * 4) as usize);
    for pixel in pixels.as_chunks_mut::<4>().0 {
        match rng.below(4) {
            0 => {
                pixel[3] = 255;
                reach.hit("opaque");
            }
            1 => {
                *pixel = [255, 255, 255, 0];
                reach.hit("white clear");
            }
            2 => {
                pixel[3] = 0;
                reach.hit("colored clear");
            }
            _ => {
                pixel[3] = rng.range(1..=254);
                reach.hit("translucent");
            }
        }
    }
    (width, height, pixels)
}

/// 同じ画素を PNG と BMP に保存し、読み戻した 2 つを返す。
fn both(name: &str, pixels: &[u8], width: i32, height: i32) -> (Vec<u8>, Vec<u8>) {
    let mut decoded = Vec::new();
    for extension in ["png", "bmp"] {
        let path = scratch(&format!("differential-{name}.{extension}"));
        encode(&path, pixels, width, height).unwrap();
        let (w, h, back) = decode(&path).expect("the file reads back");
        assert_eq!((w, h), (width, height), "{extension}");
        fs::remove_file(&path).unwrap();
        decoded.push(back);
    }
    let bmp = decoded.pop().unwrap();
    (decoded.pop().unwrap(), bmp)
}

#[test]
fn a_bmp_file_shows_the_png_file_on_white() {
    let name = "a_bmp_file_shows_the_png_file_on_white";
    let mut reach = Reach::new(&["opaque", "white clear", "colored clear", "translucent"]);
    let cases = rng::check(name, 100, |rng| {
        let (width, height, pixels) = mixed(rng, &mut reach);
        rng.note(format!("{width}x{height}"));
        let (png, bmp) = both(name, &pixels, width, height);
        assert!(
            png == pixels,
            "the PNG file keeps every pixel, transparency included"
        );
        let expected = pixels.as_chunks::<4>().0.iter();
        for (index, (got, &pixel)) in bmp.as_chunks::<4>().0.iter().zip(expected).enumerate() {
            let (x, y) = (index as i32 % width, index as i32 / width);
            assert_eq!(*got, on_white(pixel), "{pixel:?} at {x},{y}");
        }
    });
    reach.finish(name, cases);
}

#[test]
fn a_highlighter_mark_outside_a_freeform_shape_is_light_in_a_bmp_file() {
    // 左半分が切り取った形（不透明な灰色）、右半分がその外側（白の透明）。その上を蛍光ペンで横切る。
    let (width, height) = (20, 20);
    let mut pixels = Vec::new();
    for _ in 0..height {
        for x in 0..width {
            pixels.extend(if x < 10 {
                [128, 128, 128, 255]
            } else {
                [255, 255, 255, 0]
            });
        }
    }
    let yellow = annotate::COLORS[1];
    annotate::draw(
        &mut pixels,
        width,
        height,
        &[Mark {
            highlight: true,
            color: yellow,
            radius: annotate::HIGHLIGHTER_RADII[0],
            points: vec![(0.5, 5.5), (19.5, 5.5)],
        }],
    );
    let at = |x: usize, y: usize| (y * width as usize + x) * 4;
    let outside = at(15, 5);
    // 透明な画素に 45% の蛍光ペン: アルファは 0.45 * 255 = 114.75 → 115、色は黄色（#FFE600）そのもの。
    assert_eq!(pixels[outside..outside + 4], [0x00, 0xe6, 0xff, 115]);
    let (png, bmp) = both("highlighter", &pixels, width, height);
    assert_eq!(png[outside..outside + 4], [0x00, 0xe6, 0xff, 115]);
    // 白の上に重ねると、B = 255 * 140 / 255 = 140、G = (230 * 115 + 255 * 140) / 255 = 243.7 → 244、
    // R = 255。濃い黄色（0, 230, 255）のままではない。
    assert_eq!(bmp[outside..outside + 4], [140, 244, 255, 255]);
    // 形の内側は不透明なので、BMP でも PNG と同じ色。
    let inside = at(5, 5);
    assert_eq!(bmp[inside..inside + 4], png[inside..inside + 4]);
    // 線（半径 6、y = 5.5）の届かない外側は白。右下の画素の中心は線から 14 離れている。
    let corner = at(19, 19);
    assert_eq!(bmp[corner..corner + 4], [255, 255, 255, 255]);
}
