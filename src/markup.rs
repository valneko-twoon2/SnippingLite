//! クイックマークアップ: ツールバーのペンのボタンがオンのとき、切り取った範囲をその場に残し、
//! ペンや蛍光ペンで書き込んでから「キャプチャ」で確定する（Snipping Tool と同じ流れ）。

use super::*;
use crate::annotate::{self, Mark};

mod bar;
pub(crate) use bar::{bar_proc, BAR};

/// 書き込みの道具。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tool {
    Pen,
    Highlighter,
    Eraser,
}

/// 消しゴムが線に届く距離（DIP）。
const ERASER_REACH: f32 = 8.0;

pub(crate) struct Session {
    /// 切り取った画像を置く位置（オーバーレイの座標）。画像と同じ大きさ。
    area: RECT,
    /// 切り取った画像（上から下へ並ぶ BGRA、ストレートアルファ）。
    pixels: Vec<u8>,
    width: i32,
    height: i32,
    /// 自由形式の切り取り。まっすぐな辺がないので境界線を描かない。
    shaped: bool,
    marks: Vec<Mark>,
    /// 元に戻す・やり直すときに戻る、書き込みの一覧。
    undo: Vec<Vec<Mark>>,
    redo: Vec<Vec<Mark>>,
    tool: Tool,
    /// 描いている途中の線。
    stroke: Option<Mark>,
    /// 消しゴムでなぞっている途中。なぞり始める前の書き込みを持つ。
    erasing: Option<Vec<Mark>>,
    /// 確定した書き込みを重ねた画像。
    base: Vec<u8>,
    /// 画面に出す画像。透明な部分は暗くした背景と重ねてある。
    view: ui::Canvas,
    /// バーのボタン（番号、なければ -1）。
    hover: i32,
    pressed: i32,
}

/// 書き込みを始めるか。録画には使わない。
pub(crate) unsafe fn wanted() -> bool {
    state().markup && !state().video
}

/// 切り取った画像を `at`（オーバーレイの座標）に残して書き込みを始める。
/// 背景や表示用の画像を用意できなければ、書き込まずにそのまま届ける。
pub(crate) unsafe fn begin(at: POINT, pixels: Vec<u8>, width: i32, height: i32, shaped: bool) {
    let s = state();
    ShowWindow(s.toolbar, SW_HIDE);
    if s.dim_canvas.is_none() {
        // ライブ表示では、ここで画面を止めて暗くした背景を作る。
        ShowWindow(s.overlay, SW_HIDE);
        DwmFlush();
        let frozen = freeze_screen(s.origin.x, s.origin.y, s.width, s.height);
        s.live = None;
        set_layered(s.overlay, false);
        if !frozen {
            cancel();
            deliver_snip(pixels, width, height, shaped);
            return;
        }
    }
    let Some(view) = ui::Canvas::new(width, height) else {
        cancel();
        deliver_snip(pixels, width, height, shaped);
        return;
    };
    s.markup_session = Some(Box::new(Session {
        area: RECT {
            left: at.x,
            top: at.y,
            right: at.x + width,
            bottom: at.y + height,
        },
        base: pixels.clone(),
        pixels,
        width,
        height,
        shaped,
        marks: Vec::new(),
        undo: Vec::new(),
        redo: Vec::new(),
        tool: Tool::Pen,
        stroke: None,
        erasing: None,
        view,
        hover: -1,
        pressed: -1,
    }));
    render_all();
    s.dragging = false;
    s.hover_window = None;
    InvalidateRect(s.overlay, null(), 0);
    ShowWindow(s.overlay, SW_SHOW);
    bar::show();
    SetForegroundWindow(s.overlay);
    SetFocus(s.overlay);
}

/// 書き込みを重ねた画像を届けて、切り取りを終える。
pub(crate) unsafe fn capture() {
    let Some(session) = state().markup_session.take() else {
        return;
    };
    let Session {
        base,
        width,
        height,
        shaped,
        ..
    } = *session;
    cancel();
    deliver_snip(base, width, height, shaped);
}

pub(crate) unsafe fn set_tool(tool: Tool) {
    if let Some(session) = state().markup_session.as_mut() {
        session.tool = tool;
        InvalidateRect(state().markup_bar, null(), 0);
    }
}

/// 書き込みを `marks` に替え、元に戻せるように今の書き込みを `undo` に積む。
unsafe fn change(marks: Vec<Mark>) {
    let Some(session) = state().markup_session.as_mut() else {
        return;
    };
    let before = std::mem::replace(&mut session.marks, marks);
    session.undo.push(before);
    session.redo.clear();
    render_all();
}

pub(crate) unsafe fn undo() {
    let Some(session) = state().markup_session.as_mut() else {
        return;
    };
    if let Some(marks) = session.undo.pop() {
        let current = std::mem::replace(&mut session.marks, marks);
        session.redo.push(current);
        render_all();
    }
}

pub(crate) unsafe fn redo() {
    let Some(session) = state().markup_session.as_mut() else {
        return;
    };
    if let Some(marks) = session.redo.pop() {
        let current = std::mem::replace(&mut session.marks, marks);
        session.undo.push(current);
        render_all();
    }
}

/// すべての書き込みを消す（元に戻せる）。
pub(crate) unsafe fn clear() {
    if state()
        .markup_session
        .as_ref()
        .is_some_and(|s| !s.marks.is_empty())
    {
        change(Vec::new());
    }
}

/// 画像の座標で、線 `mark` が `(x, y)` から `reach` 以内を通るか。
fn touches(mark: &Mark, (x, y): (f32, f32), reach: f32) -> bool {
    let limit = (mark.radius + reach).powi(2);
    let near = |(ax, ay): (f32, f32), (bx, by): (f32, f32)| {
        let (dx, dy) = (bx - ax, by - ay);
        let length = dx * dx + dy * dy;
        let t = if length == 0.0 {
            0.0
        } else {
            (((x - ax) * dx + (y - ay) * dy) / length).clamp(0.0, 1.0)
        };
        (ax + dx * t - x).powi(2) + (ay + dy * t - y).powi(2) <= limit
    };
    match mark.points.as_slice() {
        [] => false,
        [only] => near(*only, *only),
        points => points.windows(2).any(|pair| near(pair[0], pair[1])),
    }
}

/// オーバーレイの座標を画像の座標（画素の中心）にする。
fn image_point(area: RECT, p: POINT) -> (f32, f32) {
    (
        (p.x - area.left) as f32 + 0.5,
        (p.y - area.top) as f32 + 0.5,
    )
}

/// 確定した書き込みをすべて重ね直し、画面の画像も作り直す。
unsafe fn render_all() {
    let s = state();
    let Some(session) = s.markup_session.as_mut() else {
        return;
    };
    session.base = session.pixels.clone();
    annotate::draw(
        &mut session.base,
        session.width,
        session.height,
        &session.marks,
    );
    let whole = RECT {
        left: 0,
        top: 0,
        right: session.width,
        bottom: session.height,
    };
    let base = std::mem::take(&mut session.base);
    show_region(session, s.dim_canvas.as_mut(), whole, &base);
    session.base = base;
    InvalidateRect(s.overlay, &session.area, 0);
    InvalidateRect(s.markup_bar, null(), 0);
}

/// 描いている途中の線だけを描き足す。線を囲む範囲だけを作り直すので、大きな画像でも軽い。
unsafe fn render_stroke() {
    let s = state();
    let Some(session) = s.markup_session.as_mut() else {
        return;
    };
    let Some(stroke) = session.stroke.as_ref() else {
        return;
    };
    let reach = stroke.radius + 2.0;
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in &stroke.points {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    let region = (
        (x0 - reach).floor() as i32,
        (y0 - reach).floor() as i32,
        (x1 + reach).ceil() as i32,
        (y1 + reach).ceil() as i32,
    );
    let Some((width, height, mut part)) =
        annotate::crop(&session.base, session.width, session.height, region)
    else {
        return;
    };
    let (left, top) = (region.0.max(0), region.1.max(0));
    let moved = Mark {
        points: stroke
            .points
            .iter()
            .map(|&(x, y)| (x - left as f32, y - top as f32))
            .collect(),
        ..stroke.clone()
    };
    annotate::draw(&mut part, width, height, &[moved]);
    let rect = RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    };
    show_region(session, s.dim_canvas.as_mut(), rect, &part);
    let a = session.area;
    let dirty = RECT {
        left: a.left + rect.left,
        top: a.top + rect.top,
        right: a.left + rect.right,
        bottom: a.top + rect.bottom,
    };
    InvalidateRect(s.overlay, &dirty, 0);
}

/// `shown`（`region` の大きさの BGRA）を画面の画像に書く。透明な画素は背景と重ねる。
fn show_region(session: &mut Session, dim: Option<&mut ui::Canvas>, region: RECT, shown: &[u8]) {
    let (dim_width, dim_height, dim) = match dim {
        Some(canvas) => (canvas.width, canvas.height, Some(canvas.pixels())),
        None => (0, 0, None),
    };
    let area = session.area;
    let width = (region.right - region.left) as usize;
    let view_width = session.width as usize;
    let view = session.view.pixels();
    for y in region.top..region.bottom {
        for x in region.left..region.right {
            let at = ((y - region.top) as usize * width + (x - region.left) as usize) * 4;
            let [b, g, r, a] = [shown[at], shown[at + 1], shown[at + 2], shown[at + 3]];
            let (sx, sy) = (area.left + x, area.top + y);
            let behind = dim
                .as_ref()
                .filter(|_| sx >= 0 && sy >= 0 && sx < dim_width && sy < dim_height)
                .map_or(0, |d| d[(sy * dim_width + sx) as usize]);
            let mix = |value: u8, shift: u32| {
                let under = (behind >> shift) & 0xff;
                (value as u32 * a as u32 + under * (255 - a as u32) + 127) / 255
            };
            view[y as usize * view_width + x as usize] =
                0xff000000 | mix(r, 16) << 16 | mix(g, 8) << 8 | mix(b, 0);
        }
    }
}

/// 書き込み中のオーバーレイを描く: 暗くした背景、その上に書き込んだ画像と白い枠。
unsafe fn paint(dc: HDC, dirty: RECT) {
    let s = state();
    let (Some(session), Some(dim)) = (s.markup_session.as_ref(), s.dim_canvas.as_ref()) else {
        return;
    };
    let a = session.area;
    let saved = SaveDC(dc);
    ExcludeClipRect(dc, a.left, a.top, a.right, a.bottom);
    BitBlt(
        dc,
        dirty.left,
        dirty.top,
        dirty.right - dirty.left,
        dirty.bottom - dirty.top,
        dim.dc(),
        dirty.left,
        dirty.top,
        SRCCOPY,
    );
    RestoreDC(dc, saved);
    BitBlt(
        dc,
        a.left,
        a.top,
        session.width,
        session.height,
        session.view.dc(),
        0,
        0,
        SRCCOPY,
    );
    let t = (s.scale * 2.0).round().max(1.0) as i32;
    let white = GetStockObject(WHITE_BRUSH) as HBRUSH;
    for edge in [
        (a.left - t, a.top - t, a.right + t, a.top),
        (a.left - t, a.bottom, a.right + t, a.bottom + t),
        (a.left - t, a.top, a.left, a.bottom),
        (a.right, a.top, a.right + t, a.bottom),
    ] {
        let r = RECT {
            left: edge.0,
            top: edge.1,
            right: edge.2,
            bottom: edge.3,
        };
        FillRect(dc, &r, white);
    }
}

/// 書き込み中のオーバーレイへのメッセージ。扱わないものは `None` を返す。
pub(crate) unsafe fn overlay_message(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> Option<LRESULT> {
    let session = state().markup_session.as_mut()?;
    let p = point(lp);
    match msg {
        WM_LBUTTONDOWN => {
            let inside = p.x >= session.area.left
                && p.y >= session.area.top
                && p.x < session.area.right
                && p.y < session.area.bottom;
            if inside {
                SetCapture(hwnd);
                let at = image_point(session.area, p);
                if session.tool == Tool::Eraser {
                    session.erasing = Some(session.marks.clone());
                    erase(at);
                } else {
                    let (color, thickness) = config::load_pen();
                    let highlight = session.tool == Tool::Highlighter;
                    let radii = if highlight {
                        annotate::HIGHLIGHTER_RADII
                    } else {
                        annotate::PEN_RADII
                    };
                    session.stroke = Some(Mark {
                        highlight,
                        color: if highlight {
                            annotate::COLORS[1]
                        } else {
                            annotate::COLORS[color]
                        },
                        radius: radii[thickness] * state().scale,
                        points: vec![at],
                    });
                    render_stroke();
                }
            }
            Some(0)
        }
        WM_MOUSEMOVE => {
            let at = image_point(session.area, p);
            if let Some(stroke) = session.stroke.as_mut() {
                stroke.points.push(at);
                render_stroke();
            } else if session.erasing.is_some() {
                erase(at);
            }
            Some(0)
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            if let Some(stroke) = session.stroke.take() {
                let mut marks = session.marks.clone();
                marks.push(stroke);
                change(marks);
            } else if let Some(before) = session.erasing.take() {
                // なぞった間に消した線は、まとめて 1 回で元に戻せる。
                if before.len() != session.marks.len() {
                    session.undo.push(before);
                    session.redo.clear();
                    InvalidateRect(state().markup_bar, null(), 0);
                }
            }
            Some(0)
        }
        WM_KEYDOWN => {
            let control = GetKeyState(VK_CONTROL as i32) < 0;
            match wp as u16 {
                VK_ESCAPE => cancel(),
                VK_RETURN => capture(),
                key if control && key == b'C' as u16 => capture(),
                key if control && key == b'Z' as u16 => undo(),
                key if control && key == b'Y' as u16 => redo(),
                _ => {}
            }
            Some(0)
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint(dc, ps.rcPaint);
            EndPaint(hwnd, &ps);
            Some(0)
        }
        _ => None,
    }
}

/// 消しゴムが触れた線を消す。
unsafe fn erase(at: (f32, f32)) {
    let reach = ERASER_REACH * state().scale;
    let Some(session) = state().markup_session.as_mut() else {
        return;
    };
    let before = session.marks.len();
    session.marks.retain(|mark| !touches(mark, at, reach));
    if session.marks.len() != before {
        render_all();
    }
}

#[cfg(test)]
mod tests;
