//! 差分テスト: 同じ切り取りを別の経路で作り、結果どうしを比べる。
//!
//! - 矩形と自由形式: 静止した画面（無作為な模様）から、同じ範囲を矩形で切り取った画像と、
//!   その範囲を囲む自由形式の形で切り取った画像を比べる。形の内側は矩形と同じ画素で不透明、
//!   外側は白の透明（`[255, 255, 255, 0]`）。矩形と同じ形の自由形式は、矩形と全く同じ画像になる。
//!   どちらも画面の模様の同じ場所の色を持つ。
//! - 枠線: docs/behavior.md は「枠線は矩形、ウィンドウ、全画面の内側に付き、自由形式には付かない」と言う。
//!   枠線ありの矩形は、枠の幅の内側（枠より中の部分）で枠線なしの矩形と同じで、枠は選んだ色。
//!   自由形式は枠線の設定で変わらない。
//! - 届け先: 1 回の切り取りを、クリップボード、保存したファイル、履歴へ届け、3 つの画素を比べる。
//!
//! 画面は覆わない。オーバーレイを出さずに、静止した画面のビットマップへ模様を置いてから、
//! マウスを離したときと同じ `finish` を呼ぶ。結果は履歴（既定でオン）から読み戻す。

use super::*;
use crate::test_support::{
    distance_to_line,
    rng::{self, Reach, Rng},
};

/// 静止した画面の代わり。上から下への 0x00RRGGBB。
struct Pattern {
    width: i32,
    height: i32,
    pixels: Vec<u32>,
}

impl Pattern {
    fn random(rng: &mut Rng) -> Pattern {
        let (width, height) = (rng.range(8..=64), rng.range(8..=48));
        let pixels = (0..width * height)
            .map(|_| rng.next_u64() as u32 & 0x00ff_ffff)
            .collect();
        Pattern {
            width,
            height,
            pixels,
        }
    }

    /// 画面の (x, y) の色を、切り取った画像と同じ BGRA の順で。
    fn bgra(&self, x: i32, y: i32) -> [u8; 4] {
        let value = self.pixels[(y * self.width + x) as usize];
        let [b, g, r, _] = value.to_le_bytes();
        [b, g, r, 255]
    }

    /// 静止した画面のビットマップを作り、模様を描く（`freeze_screen` が画面を写すのと同じ場所）。
    unsafe fn freeze(&self) {
        let dc = GetDC(null_mut());
        assert!(
            store_bitmap(dc, self.width, self.height),
            "the screen bitmap"
        );
        ReleaseDC(null_mut(), dc);
        let mut canvas = ui::Canvas::new(self.width, self.height).expect("a canvas");
        canvas.pixels().copy_from_slice(&self.pixels);
        let copied = BitBlt(
            s().screen_dc,
            0,
            0,
            self.width,
            self.height,
            canvas.dc(),
            0,
            0,
            SRCCOPY,
        );
        assert!(copied != 0, "the pattern is on the frozen screen");
        GdiFlush();
    }
}

/// 切り取った画像。上から下への BGRA。
struct Picture {
    width: i32,
    height: i32,
    pixels: Vec<u8>,
}

impl Picture {
    fn at(&self, x: i32, y: i32) -> [u8; 4] {
        pixel(&self.pixels, self.width, x, y)
    }
}

/// 枠線の設定: (付けるか, 色の番号, 太さの番号)。
type Outline = (bool, usize, usize);

/// 静止した画面から `rect` を切り取り、履歴に入った画像を返す。`points` があれば自由形式。
/// 次の切り取りと混ざらないよう、履歴は空に戻す。
unsafe fn snip(
    pattern: &Pattern,
    rect: RECT,
    points: Option<&[POINT]>,
    outline: Outline,
) -> Picture {
    (s().outline, s().outline_color, s().outline_width) = outline;
    pattern.freeze();
    s().points = points.map(<[POINT]>::to_vec).unwrap_or_default();
    finish(rect, points.is_some());
    assert!(!s().active && s().screen_dc.is_null(), "the snip is over");
    let entries = history::list();
    assert_eq!(entries.len(), 1, "the capture reached the history");
    let (width, height, pixels) =
        imaging::decode(&entries[0].image).expect("the capture is a picture");
    history::clear();
    Picture {
        width,
        height,
        pixels,
    }
}

/// 画面の中の、幅と高さが 1 以上の範囲。
fn area(rng: &mut Rng, pattern: &Pattern) -> RECT {
    let left = rng.range(0..pattern.width);
    let top = rng.range(0..pattern.height);
    RECT {
        left,
        top,
        right: rng.range(left + 1..=pattern.width),
        bottom: rng.range(top + 1..=pattern.height),
    }
}

/// 奇偶規則（GDI の ALTERNATE と同じ）で、点が多角形の内側にあるか。
fn inside(polygon: &[POINT], (x, y): (f64, f64)) -> bool {
    let mut inside = false;
    for (index, a) in polygon.iter().enumerate() {
        let b = polygon[(index + 1) % polygon.len()];
        let (ax, ay, bx, by) = (a.x as f64, a.y as f64, b.x as f64, b.y as f64);
        if (ay > y) != (by > y) && x < ax + (y - ay) * (bx - ax) / (by - ay) {
            inside = !inside;
        }
    }
    inside
}

/// 矩形と同じ形の自由形式は矩形と同じ画像になり、枠線は矩形の縁にだけ付く。
unsafe fn rectangle_case(rng: &mut Rng, pattern: &Pattern, outline: Outline, reach: &mut Reach) {
    let rect = area(rng, pattern);
    let mut corners = vec![
        POINT {
            x: rect.left,
            y: rect.top,
        },
        POINT {
            x: rect.right,
            y: rect.top,
        },
        POINT {
            x: rect.right,
            y: rect.bottom,
        },
        POINT {
            x: rect.left,
            y: rect.bottom,
        },
    ];
    // どの角から、どちら回りになぞっても同じ形。
    corners.rotate_left(rng.below(4));
    if rng.chance(0.5) {
        corners.reverse();
    }
    rng.note(format!(
        "rectangle {:?} traced as {:?}, outline {outline:?}",
        (rect.left, rect.top, rect.right, rect.bottom),
        corners.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>()
    ));
    let plain = snip(pattern, rect, None, (false, 0, 0));
    let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
    assert_eq!((plain.width, plain.height), (width, height));
    for y in 0..height {
        for x in 0..width {
            assert_eq!(
                plain.at(x, y),
                pattern.bgra(rect.left + x, rect.top + y),
                "the rectangle shows the screen at {x},{y}"
            );
        }
    }
    let free = snip(pattern, rect, Some(&corners), outline);
    assert!(
        free.width == width && free.height == height && free.pixels == plain.pixels,
        "the same shape traced freehand gives the same picture"
    );
    reach.hit("rectangle");
    if !outline.0 {
        return;
    }
    reach.hit("outline");
    let framed = snip(pattern, rect, None, outline);
    let border = annotate::OUTLINE_WIDTHS[outline.2];
    let color = annotate::COLORS[outline.1];
    let painted = [color[2], color[1], color[0], 255];
    for y in 0..height {
        for x in 0..width {
            let edge = x < border || y < border || x >= width - border || y >= height - border;
            let expected = if edge { painted } else { plain.at(x, y) };
            assert_eq!(framed.at(x, y), expected, "edge={edge} at {x},{y}");
        }
    }
}

/// 自由形式は、形の内側で同じ範囲の矩形と同じ画素を持ち、外側は白の透明になる。
unsafe fn polygon_case(rng: &mut Rng, pattern: &Pattern, outline: Outline, reach: &mut Reach) {
    // 対角の 2 点で、外接する矩形に幅と高さを持たせる。ほかの点は画面のどこでもよい。
    let corner = area(rng, pattern);
    let mut points = vec![
        POINT {
            x: corner.left,
            y: corner.top,
        },
        POINT {
            x: corner.right,
            y: corner.bottom,
        },
    ];
    for _ in 0..rng.range(1..=6) {
        points.push(POINT {
            x: rng.range(0..=pattern.width),
            y: rng.range(0..=pattern.height),
        });
    }
    rng.shuffle(&mut points);
    rng.note(format!(
        "shape {:?} on {}x{}, outline {outline:?}",
        points.iter().map(|p| (p.x, p.y)).collect::<Vec<_>>(),
        pattern.width,
        pattern.height
    ));
    // オーバーレイがマウスを離したときに求めるのと同じ、点を囲む範囲。
    let low = |pick: fn(&POINT) -> i32| points.iter().map(pick).min().unwrap();
    let high = |pick: fn(&POINT) -> i32| points.iter().map(pick).max().unwrap();
    let rect = RECT {
        left: low(|p| p.x),
        top: low(|p| p.y),
        right: high(|p| p.x),
        bottom: high(|p| p.y),
    };
    let plain = snip(pattern, rect, None, (false, 0, 0));
    let free = snip(pattern, rect, Some(&points), outline);
    assert_eq!((free.width, free.height), (plain.width, plain.height));
    let mut closed: Vec<(f32, f32)> = points.iter().map(|p| (p.x as f32, p.y as f32)).collect();
    closed.push(closed[0]);
    let (mut opaque, mut clear) = (0, 0);
    for y in 0..free.height {
        for x in 0..free.width {
            let got = free.at(x, y);
            match got[3] {
                255 => assert_eq!(got, plain.at(x, y), "inside the shape at {x},{y}"),
                0 => assert_eq!(got, [255, 255, 255, 0], "outside the shape at {x},{y}"),
                alpha => panic!("a pixel of alpha {alpha} at {x},{y}"),
            }
            // 画素の中心が形の縁から 1 画素より離れていれば、内か外かは縁の扱いによらない。
            let center = ((rect.left + x) as f64 + 0.5, (rect.top + y) as f64 + 0.5);
            let far = (center.0 as f32, center.1 as f32);
            if distance_to_line(&closed, far).unwrap() > 1.0 {
                let within = inside(&points, center);
                assert_eq!(got[3] == 255, within, "inside={within} at {x},{y}");
                if within {
                    opaque += 1;
                } else {
                    clear += 1;
                }
            }
        }
    }
    reach.hit("shape");
    if opaque > 0 {
        reach.hit("inside");
    }
    if clear > 0 {
        reach.hit("outside");
    }
}

#[test]
fn a_freeform_shape_keeps_the_rectangle_pixels_inside_it_and_clears_the_rest() {
    let _desk = Desk::start_with(|settings| {
        settings.copy = false;
        settings.save = false;
        settings.history = true;
        settings.notify = false;
    });
    let mut reach = Reach::new(&["rectangle", "outline", "shape", "inside", "outside"]);
    let name = "a_freeform_shape_keeps_the_rectangle_pixels_inside_it_and_clears_the_rest";
    let cases = rng::check(name, 60, |rng| {
        let pattern = Pattern::random(rng);
        let outline = (
            rng.chance(0.5),
            rng.below(annotate::COLORS.len()),
            rng.below(annotate::OUTLINE_WIDTHS.len()),
        );
        // SAFETY: the state and the bitmaps belong to this test, on this thread.
        unsafe {
            if rng.chance(0.4) {
                rectangle_case(rng, &pattern, outline, &mut reach);
            } else {
                polygon_case(rng, &pattern, outline, &mut reach);
            }
        }
    });
    reach.finish(name, cases);
}

/// 届け先に送る画像。不透明な画素、自由形式の外側（白の透明）、自由形式の外側に書き込んだ
/// 半透明の画素が混ざる。
fn delivered(rng: &mut Rng) -> Picture {
    let (width, height) = (rng.range(1..=40), rng.range(1..=30));
    let mut pixels = rng.bytes((width * height * 4) as usize);
    for pixel in pixels.as_chunks_mut::<4>().0 {
        match rng.below(6) {
            0 => *pixel = [255, 255, 255, 0],
            1 => pixel[3] = rng.range(1..=254),
            _ => pixel[3] = 255,
        }
    }
    Picture {
        width,
        height,
        pixels,
    }
}

#[test]
#[ignore = "Replaces the clipboard content"]
fn one_capture_reaches_the_clipboard_the_file_and_the_history_with_the_same_pixels() {
    let desk = Desk::start_with(|settings| {
        settings.copy = true;
        settings.save = true;
        settings.ask = false;
        settings.history = true;
        settings.notify = false;
        settings.outline = false;
    });
    let name = "one_capture_reaches_the_clipboard_the_file_and_the_history_with_the_same_pixels";
    rng::check(name, 12, |rng| {
        let picture = delivered(rng);
        let (width, height) = (picture.width, picture.height);
        rng.note(format!("{width}x{height}"));
        s().save = true;
        // SAFETY: the state belongs to this test, on this thread.
        // ふつうの確定と同じく、自動コピーの設定（ここではオン）に従って届ける。
        unsafe { deliver_capture(picture.pixels.clone(), width, height, s().copy) };
        let (clip_width, clip_height, clip) = clipboard_image().expect("an image on the clipboard");
        assert_eq!((clip_width, clip_height), (width, height));
        assert!(clip == picture.pixels, "the clipboard holds the pixels");
        let files = shots(&desk);
        assert_eq!(files.len(), 1, "one file was saved");
        let saved = imaging::decode(&files[0]).expect("the saved file is a picture");
        assert!(saved == (width, height, picture.pixels.clone()), "the file");
        let entry = history::list().remove(0);
        let kept = imaging::decode(&entry.image).expect("the history has the picture");
        assert!(kept == saved, "the history has the same pixels as the file");
        assert_eq!(
            std::fs::read(&entry.image).unwrap(),
            std::fs::read(&files[0]).unwrap()
        );
        std::fs::remove_file(&files[0]).unwrap();
        history::clear();
        // 保存しないときは、履歴が自分で画像を書く。それでも同じ画素になる。
        s().save = false;
        unsafe { deliver_capture(picture.pixels.clone(), width, height, s().copy) };
        let entry = history::list().remove(0);
        let kept = imaging::decode(&entry.image).expect("the history has the picture");
        assert!(kept == (width, height, picture.pixels), "the history alone");
        assert!(shots(&desk).is_empty());
        history::clear();
    });
}
