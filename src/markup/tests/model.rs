//! 書き込みのモデルベーステスト。線を描く、消しゴムで消す・なぞる、元に戻す、やり直す、
//! すべて消去、道具とペンの切り替えを無作為な順に本物のオーバーレイへ送り、同じ操作を小さな
//! モデル（書き込みの一覧と、元に戻す・やり直すのスタック）にも当てて、1 手ごとに一致を確かめる。
//! ウィンドウは表示しないままメッセージを直接送るので、画面は覆わない。

use super::*;
use crate::test_support::rng::{self, Reach, Rng};

/// 書き込みの仕様を数行で書いたもの。線が消しゴムに触れるかどうかは、本物の `touches` で
/// 決める（`touches` 自体は properties.rs で、別の方法で求めた距離と比べてある）。
struct Model {
    marks: Vec<Mark>,
    undo: Vec<Vec<Mark>>,
    redo: Vec<Vec<Mark>>,
    tool: Tool,
    /// 保存してあるペンの色と太さ（番号）。
    pen: (usize, usize),
}

impl Model {
    fn change(&mut self, marks: Vec<Mark>) {
        let before = std::mem::replace(&mut self.marks, marks);
        self.undo.push(before);
        self.redo.clear();
    }

    /// 線を描き終えると、それが書き込みの最後に加わり、1 回で元に戻せる。
    fn draw(&mut self, points: Vec<(f32, f32)>) {
        let highlight = self.tool == Tool::Highlighter;
        let (radii, color) = if highlight {
            (annotate::HIGHLIGHTER_RADII, annotate::COLORS[1])
        } else {
            (annotate::PEN_RADII, annotate::COLORS[self.pen.0])
        };
        let mut marks = self.marks.clone();
        marks.push(Mark {
            highlight,
            color,
            radius: radii[self.pen.1] * s().scale,
            points,
        });
        self.change(marks);
    }

    /// なぞった点のどれかから届く線が消える。消えた線があれば、まとめて 1 回で元に戻せる。
    fn erase(&mut self, points: &[(f32, f32)]) -> bool {
        let reach = ERASER_REACH * s().scale;
        let kept: Vec<Mark> = self
            .marks
            .iter()
            .filter(|mark| !points.iter().any(|&p| touches(mark, p, reach)))
            .cloned()
            .collect();
        let erased = kept.len() != self.marks.len();
        if erased {
            self.change(kept);
        }
        erased
    }

    fn undo(&mut self) -> bool {
        let Some(marks) = self.undo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.marks, marks);
        self.redo.push(current);
        true
    }

    fn redo(&mut self) -> bool {
        let Some(marks) = self.redo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.marks, marks);
        self.undo.push(current);
        true
    }

    fn clear(&mut self) -> bool {
        let any = !self.marks.is_empty();
        if any {
            self.change(Vec::new());
        }
        any
    }
}

#[derive(Debug)]
enum Action {
    /// 最初の点で押し、残りの点へ動かし、最後の点で離す。1 点だけならクリック。
    Drag(Vec<(i32, i32)>),
    /// ボタンを押さずに動かしてから離す（押していないので何も起きない）。
    Hover((i32, i32)),
    /// バーのボタン。
    Press(i32),
    /// 履歴の編集で選んだペンの色と太さ（番号）を保存する。
    Pen(usize, usize),
}

/// 画面の座標を画像の座標（画素の中心）にする。
fn pixel_center(area: RECT, (x, y): (i32, i32)) -> (f32, f32) {
    ((x - area.left) as f32 + 0.5, (y - area.top) as f32 + 0.5)
}

fn inside(area: RECT, (x, y): (i32, i32)) -> bool {
    x >= area.left && y >= area.top && x < area.right && y < area.bottom
}

/// 押す点: たいていは範囲の中、ときどき今ある線のそば（消しゴムが届くように）や範囲の外。
fn spot(rng: &mut Rng, area: RECT, model: &Model) -> (i32, i32) {
    match rng.below(20) {
        0..=13 => (
            rng.range(area.left..area.right),
            rng.range(area.top..area.bottom),
        ),
        14..=16 if !model.marks.is_empty() => {
            let mark = rng.pick(&model.marks);
            let &(x, y) = rng.pick(&mark.points);
            (
                area.left + x.floor() as i32 + rng.range(-14..=14),
                area.top + y.floor() as i32 + rng.range(-14..=14),
            )
        }
        _ => (
            rng.range(area.left - 24..area.right + 24),
            rng.range(area.top - 24..area.bottom + 24),
        ),
    }
}

fn action(rng: &mut Rng, area: RECT, model: &Model) -> Action {
    match rng.weighted(&[40, 4, 12, 14, 10, 4, 4]) {
        0 => {
            let mut points = vec![spot(rng, area, model)];
            for _ in 0..rng.range(0..=6) {
                let (x, y) = points[points.len() - 1];
                points.push((x + rng.range(-16..=16), y + rng.range(-16..=16)));
            }
            Action::Drag(points)
        }
        1 => Action::Hover(spot(rng, area, model)),
        2 => Action::Press(*rng.pick(&[PEN, HIGHLIGHTER, ERASER])),
        3 => Action::Press(UNDO),
        4 => Action::Press(REDO),
        5 => Action::Press(CLEAR),
        _ => Action::Pen(
            rng.below(annotate::COLORS.len()),
            rng.below(annotate::PEN_RADII.len()),
        ),
    }
}

fn session() -> &'static mut Session {
    s().markup_session.as_mut().expect("the markup goes on")
}

fn view() -> Vec<u32> {
    session().view.pixels().to_vec()
}

/// 2 つの画像（同じ大きさ）が同じか。違えば、違う画素の数と最初の 1 つを示す。
fn assert_same<T: PartialEq + std::fmt::Debug>(actual: &[T], expected: &[T], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}");
    let differ = actual.iter().zip(expected).filter(|(a, e)| a != e).count();
    if let Some(at) = (0..actual.len()).find(|&i| actual[i] != expected[i]) {
        panic!(
            "{what}: {differ} values differ, the first at {at}: {:?} instead of {:?}",
            actual[at], expected[at]
        );
    }
}

/// 画面の画像（0xAARRGGBB）が、どの色でも 1 段階以内で同じか。
fn assert_close(actual: &[u32], expected: &[u32], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}");
    let far = |a: u32, e: u32| {
        (0..4).any(|byte| ((a >> (byte * 8)) as u8).abs_diff((e >> (byte * 8)) as u8) > 1)
    };
    if let Some(at) = (0..actual.len()).find(|&i| far(actual[i], expected[i])) {
        panic!(
            "{what}: at {at}, {:#010x} instead of {:#010x}",
            actual[at], expected[at]
        );
    }
}

fn listed(marks: &[Mark]) -> String {
    format!("{marks:?}")
}

fn history(stack: &[Vec<Mark>]) -> Vec<String> {
    stack.iter().map(|marks| listed(marks)).collect()
}

/// 本物がモデルと同じ状態にあり、画像がその書き込みから描き直したものと同じか。
fn assert_matches(model: &Model, snip: &[u8], dim: &[u32], dim_width: i32) {
    let session = session();
    assert_eq!(listed(&session.marks), listed(&model.marks), "the marks");
    assert_eq!(
        history(&session.undo),
        history(&model.undo),
        "what undo brings back"
    );
    assert_eq!(
        history(&session.redo),
        history(&model.redo),
        "what redo brings back"
    );
    assert_eq!(session.tool, model.tool);
    assert!(
        session.stroke.is_none() && session.erasing.is_none(),
        "nothing is left half done"
    );
    assert!(session.pixels == snip, "the snip itself is never drawn on");
    let (width, height) = (session.width, session.height);
    let mut expected = snip.to_vec();
    annotate::draw(&mut expected, width, height, &model.marks);
    assert_same(
        &session.base,
        &expected,
        "the image is the snip with exactly the listed marks",
    );
    // 画面の画像: 不透明な画素はその色、透明な画素は暗くした背景のまま。
    let area = session.area;
    let base = session.base.clone();
    let shown = session.view.pixels();
    for y in 0..height {
        for x in 0..width {
            let at = (y * width + x) as usize;
            let [b, g, r, a] = [
                base[at * 4],
                base[at * 4 + 1],
                base[at * 4 + 2],
                base[at * 4 + 3],
            ];
            let expected = match a {
                255 => 0xff000000 | (r as u32) << 16 | (g as u32) << 8 | b as u32,
                0 => {
                    let (sx, sy) = (area.left + x, area.top + y);
                    0xff000000 | dim[(sy * dim_width + sx) as usize] & 0xffffff
                }
                // 半透明の画素は背景と混ぜた色になる。混ぜ方はここでは確かめない。
                _ => continue,
            };
            assert_eq!(shown[at], expected, "the screen at {x},{y}");
        }
    }
}

/// 送った操作をモデルにも当てる。到達した分岐を数える。
fn apply(desk: &Desk, action: &Action, model: &mut Model, area: RECT, reach: &mut Reach) {
    let overlay = s().overlay;
    match action {
        Action::Drag(points) => {
            let (first, last) = (points[0], points[points.len() - 1]);
            desk.mouse(overlay, WM_MOUSEMOVE, first.0, first.1);
            desk.mouse(overlay, WM_LBUTTONDOWN, first.0, first.1);
            for &(x, y) in &points[1..] {
                desk.mouse(overlay, WM_MOUSEMOVE, x, y);
            }
            let pressed = inside(area, first);
            let drawing = pressed && model.tool != Tool::Eraser;
            assert_eq!(session().stroke.is_some(), drawing, "a line is being drawn");
            let erasing = pressed && model.tool == Tool::Eraser;
            assert_eq!(session().erasing.is_some(), erasing, "the eraser is down");
            let while_drawing = view();
            desk.mouse(overlay, WM_LBUTTONUP, last.0, last.1);
            let centers: Vec<(f32, f32)> = points.iter().map(|&p| pixel_center(area, p)).collect();
            if drawing {
                model.draw(centers);
                // 描いている間に見えていた線が、離した後もそのまま残る。描いている間は線の
                // まわりだけを切り抜いて描くので、座標の丸めで色が 1 段階ずれる画素はありうる。
                assert_close(
                    &view(),
                    &while_drawing,
                    "the line looks the same once let go",
                );
                reach.hit("drew a line");
            } else if erasing {
                reach.hit(if model.erase(&centers) {
                    "erased lines"
                } else {
                    "erased nothing"
                });
            } else {
                reach.hit("pressed outside");
            }
        }
        Action::Hover((x, y)) => {
            desk.mouse(overlay, WM_MOUSEMOVE, *x, *y);
            desk.mouse(overlay, WM_LBUTTONUP, *x, *y);
            reach.hit("hovered");
        }
        &Action::Press(button) => {
            // SAFETY: the windows belong to this test.
            unsafe { press(button) };
            let label = match button {
                PEN | HIGHLIGHTER | ERASER => {
                    model.tool = match button {
                        PEN => Tool::Pen,
                        HIGHLIGHTER => Tool::Highlighter,
                        _ => Tool::Eraser,
                    };
                    "switched tools"
                }
                UNDO if model.undo() => "undid",
                UNDO => "nothing to undo",
                REDO if model.redo() => "redid",
                REDO => "nothing to redo",
                _ if model.clear() => "cleared",
                _ => "nothing to clear",
            };
            reach.hit(label);
        }
        &Action::Pen(color, thickness) => {
            config::save_pen(color, thickness);
            model.pen = (color, thickness);
            reach.hit("changed the pen");
        }
    }
}

/// 切り取った画像。自由形式なら、楕円の外は透明。
fn snip(rng: &mut Rng, width: i32, height: i32, shaped: bool) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let dx = (x as f32 + 0.5) / width as f32 - 0.5;
            let dy = (y as f32 + 0.5) / height as f32 - 0.5;
            let opaque = !shaped || dx * dx + dy * dy <= 0.25;
            let color = rng.bytes(3);
            pixels.extend([color[0], color[1], color[2], if opaque { 255 } else { 0 }]);
        }
    }
    pixels
}

/// 書き込みのバーを、表示せずに作る。バーがないと、再描画の依頼が画面全体に向かう。
fn hidden_bar() {
    // SAFETY: a hidden window of the registered bar class, owned by the overlay of this test.
    unsafe {
        s().markup_bar = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w(BAR).as_ptr(),
            w(BAR).as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            s().overlay,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
    }
    assert!(!s().markup_bar.is_null(), "the markup bar");
}

#[test]
fn random_markup_edits_match_a_model_of_the_marks_and_their_history() {
    let desk = Desk::start();
    hidden_bar();
    let mut reach = Reach::new(&[
        "drew a line",
        "pressed outside",
        "erased lines",
        "erased nothing",
        "undid",
        "nothing to undo",
        "redid",
        "nothing to redo",
        "cleared",
        "nothing to clear",
        "switched tools",
        "changed the pen",
        "hovered",
    ]);
    let cases = rng::check(
        "random_markup_edits_match_a_model_of_the_marks_and_their_history",
        100,
        |rng| {
            s().scale = *rng.pick(&SCALES);
            let pen = (
                rng.below(annotate::COLORS.len()),
                rng.below(annotate::PEN_RADII.len()),
            );
            config::save_pen(pen.0, pen.1);
            let (width, height) = (rng.range(8..=48), rng.range(8..=48));
            // 画面の端に接する範囲（画面の隅からの切り取り、画面全体）はよくある。
            let mut edge = || {
                if rng.chance(0.3) {
                    0
                } else {
                    rng.range(1..=40)
                }
            };
            let (left, top) = (edge(), edge());
            let (right, bottom) = (edge(), edge());
            let area = rect(left, top, left + width, top + height);
            let shaped = rng.chance(0.3);
            let pixels = snip(rng, width, height, shaped);
            // 暗くした背景（オーバーレイ全体）。範囲を含む大きさにする。
            let dim_width = area.right + right;
            // SAFETY: plain GDI allocations owned by the state of this test.
            let (mut dim, view) = unsafe {
                (
                    ui::Canvas::new(dim_width, area.bottom + bottom).unwrap(),
                    ui::Canvas::new(width, height).unwrap(),
                )
            };
            for pixel in dim.pixels() {
                *pixel = rng.next_u64() as u32 & 0x007f7f7f;
            }
            let behind = dim.pixels().to_vec();
            s().dim_canvas = Some(dim);
            s().markup_session = Some(Box::new(Session {
                area,
                pixels: pixels.clone(),
                width,
                height,
                shaped,
                marks: Vec::new(),
                undo: Vec::new(),
                redo: Vec::new(),
                tool: Tool::Pen,
                stroke: None,
                erasing: None,
                base: pixels.clone(),
                view,
                hover: -1,
                pressed: -1,
            }));
            // SAFETY: the session belongs to this test.
            unsafe { render_all() };
            let mut model = Model {
                marks: Vec::new(),
                undo: Vec::new(),
                redo: Vec::new(),
                tool: Tool::Pen,
                pen,
            };
            rng.note(format!(
                "scale {} pen {pen:?} area {left},{top} {width}x{height} shaped {shaped}",
                s().scale
            ));
            assert_matches(&model, &pixels, &behind, dim_width);
            for _ in 0..rng.range(1..=40) {
                let action = action(rng, area, &model);
                rng.note(format!("{action:?}"));
                apply(&desk, &action, &mut model, area, &mut reach);
                assert_matches(&model, &pixels, &behind, dim_width);
            }
            s().markup_session = None;
            s().dim_canvas = None;
        },
    );
    drop(desk);
    reach.finish("random_markup_edits_match_a_model", cases);
}
