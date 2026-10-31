//! テスト用の窓口: 書き込みの状態とバーのボタンの位置を、markup の外のテスト（モンキーテスト）
//! から読めるようにする。読むだけで、状態は変えない。

use super::*;

/// 書き込み中の切り取りのうち、外のテストが確かめる部分の写し。
#[derive(Clone, Copy)]
pub(crate) struct Look {
    /// 切り取った画像を置いた所（オーバーレイの座標）。
    pub area: RECT,
    pub width: i32,
    pub height: i32,
    pub tool: Tool,
    pub marks: usize,
    /// 描いている途中の線がある。
    pub drawing: bool,
    /// 消しゴムでなぞっている途中。
    pub erasing: bool,
}

/// 書き込み中なら、その写し。
pub(crate) unsafe fn look() -> Option<Look> {
    state().markup_session.as_ref().map(|session| Look {
        area: session.area,
        width: session.width,
        height: session.height,
        tool: session.tool,
        marks: session.marks.len(),
        drawing: session.stroke.is_some(),
        erasing: session.erasing.is_some(),
    })
}

/// バーのボタンの範囲（クライアント座標）。
pub(crate) fn button(index: i32, scale: f32) -> [f32; 4] {
    bar::button(index, scale)
}

pub(crate) const CAPTURE: i32 = bar::CAPTURE;
pub(crate) const PEN: i32 = bar::PEN;
pub(crate) const HIGHLIGHTER: i32 = bar::HIGHLIGHTER;
pub(crate) const ERASER: i32 = bar::ERASER;
pub(crate) const UNDO: i32 = bar::UNDO;
pub(crate) const REDO: i32 = bar::REDO;
pub(crate) const CLEAR: i32 = bar::CLEAR;
pub(crate) const CLOSE: i32 = bar::CLOSE;
