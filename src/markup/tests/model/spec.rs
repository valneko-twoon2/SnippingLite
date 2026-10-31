//! 書き込みの仕様を数行で書いたモデル: 書き込みの一覧と、元に戻す・やり直すのスタック。線が
//! 消しゴムに触れるかどうかは、本物の `touches` で決める（`touches` 自体は properties.rs で、
//! 別の方法で求めた距離と比べてある）。

use super::*;

pub(super) struct Model {
    pub(super) marks: Vec<Mark>,
    pub(super) undo: Vec<Vec<Mark>>,
    pub(super) redo: Vec<Vec<Mark>>,
    pub(super) tool: Tool,
    /// 保存してあるペンの色と太さ（番号）。
    pub(super) pen: (usize, usize),
}

impl Model {
    fn change(&mut self, marks: Vec<Mark>) {
        let before = std::mem::replace(&mut self.marks, marks);
        self.undo.push(before);
        self.redo.clear();
    }

    /// 線を描き終えると、それが書き込みの最後に加わり、1 回で元に戻せる。
    pub(super) fn draw(&mut self, points: Vec<(f32, f32)>) {
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

    /// なぞった道筋（押した点と、そこから順に動いた点を結ぶ線分）のどこかから届く線が消える。
    /// 消えた線があれば、まとめて 1 回で元に戻せる。
    pub(super) fn erase(&mut self, points: &[(f32, f32)]) -> bool {
        let reach = ERASER_REACH * s().scale;
        let paths: Vec<((f32, f32), (f32, f32))> = (0..points.len())
            .map(|i| (points[i.saturating_sub(1)], points[i]))
            .collect();
        let kept: Vec<Mark> = self
            .marks
            .iter()
            .filter(|mark| {
                !paths
                    .iter()
                    .any(|&(from, to)| touches(mark, from, to, reach))
            })
            .cloned()
            .collect();
        let erased = kept.len() != self.marks.len();
        if erased {
            self.change(kept);
        }
        erased
    }

    pub(super) fn undo(&mut self) -> bool {
        let Some(marks) = self.undo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.marks, marks);
        self.redo.push(current);
        true
    }

    pub(super) fn redo(&mut self) -> bool {
        let Some(marks) = self.redo.pop() else {
            return false;
        };
        let current = std::mem::replace(&mut self.marks, marks);
        self.undo.push(current);
        true
    }

    pub(super) fn clear(&mut self) -> bool {
        let any = !self.marks.is_empty();
        if any {
            self.change(Vec::new());
        }
        any
    }
}
