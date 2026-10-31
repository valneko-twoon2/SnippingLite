//! ペンと蛍光ペンのブラシ: 線の覆いを画素ごとに求め、色を重ねる。
//!
//! 覆いは、画素の中心から線（点を結んだ折れ線）までの距離 `d` で決める。`d <= 半径 - 0.5` の
//! 画素は全部、`d >= 半径 + 0.5` の画素は少しも覆わず、その間の 1 画素の幅は距離に比例させる。
//! 点を少しずつずらしながら円を押していくと、円と円の間で縁が内側へへこみ、半径の 3% ほど
//! 波打つが、線分までの距離で決めると縁はまっすぐになる。
//!
//! 計算は f64 で、画素の中心から線分の端までの差を先に取る。どの値も画像の中の位置と線の座標
//! だけで決まり、描く部分がどこかによらないので、部分に描いた画像は全体に描いた画像の同じ部分と
//! ぴったり同じになる。

use super::{opacity, Mark};

/// 覆う範囲の端を求めるときの余裕（画素）。端は f64 の丸めで 1e-6 画素ほどずれうるので、全部
/// 覆う範囲はこれだけ狭く、少しでも覆う範囲はこれだけ広く取り、間の画素は距離の式で求める。
/// こうすると、どの画素の値も、すべての画素を距離の式で求めた値と同じになる。
const SLACK: f64 = 0.01;

/// 描く部分の位置と大きさ（画像の画素で）。
#[derive(Clone, Copy)]
pub(super) struct Part {
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) width: i32,
    pub(super) height: i32,
}

/// 1 本の線を描く。`pixels` は `part` の大きさの、ストレートアルファの BGRA。
pub(super) fn stroke(pixels: &mut [u8], part: Part, mark: &Mark) {
    if mark.points.is_empty() || part.width <= 0 || part.height <= 0 {
        return;
    }
    // 1 本の線の覆いを先に集めるので、線が自分と重なっても 2 度は混ぜない。
    let mut coverage = vec![0u8; part.width as usize * part.height as usize];
    let points: Vec<(f64, f64)> = mark
        .points
        .iter()
        .map(|&(x, y)| (x as f64, y as f64))
        .collect();
    let reach = mark.radius as f64 + 0.5;
    if let [only] = points[..] {
        cover(&mut coverage, part, only, only, reach);
    }
    for pair in points.windows(2) {
        cover(&mut coverage, part, pair[0], pair[1], reach);
    }
    blend(pixels, &coverage, mark.color, opacity(mark.highlight));
}

/// 線分 `a`–`b`（画像の座標）の覆いを、`coverage` の大きいほうに残す。`reach` は半径 + 0.5 で、
/// 中心がそれより遠い画素は覆わず、`reach - 1` 以内の画素は全部覆う。
fn cover(coverage: &mut [u8], part: Part, a: (f64, f64), b: (f64, f64), reach: f64) {
    let capsule = Capsule::new(a, b);
    let width = part.width as usize;
    let rows = span(
        a.1.min(b.1) - reach,
        a.1.max(b.1) + reach,
        part.top,
        part.height,
    );
    for y in rows {
        let py = y as f64 + 0.5 - a.1;
        // 線分から `reach` 以内の形（両端の丸い帯）は凸なので、行と重なる所は 1 つの区間になる。
        // その外の画素は覆わない。
        let Some((low, high)) = capsule.chord(py, reach + SLACK) else {
            continue;
        };
        let columns = span(a.0 + low, a.0 + high, part.left, part.width);
        // 中心が `reach - 1` 以内の画素は全部覆うので、距離を求めずに埋める。
        let inside = match capsule.chord(py, reach - 1.0 - SLACK) {
            Some((low, high)) => {
                let (start, end) = (columns.start as i64, columns.end as i64);
                let first = ((a.0 + low - 0.5).ceil() as i64).clamp(start, end);
                let last = ((a.0 + high - 0.5).floor() as i64).saturating_add(1);
                first as i32..last.clamp(first, end) as i32
            }
            None => columns.start..columns.start,
        };
        let row = &mut coverage[(y - part.top) as usize * width..][..width];
        let at = |x: i32| (x - part.left) as usize;
        row[at(inside.start)..at(inside.end)].fill(255);
        for x in (columns.start..inside.start).chain(inside.end..columns.end) {
            let slot = &mut row[at(x)];
            // 前の線分で全部覆った画素は、どの線分でもそれ以上にならない。
            if *slot == 255 {
                continue;
            }
            let distance = capsule.distance(x as f64 + 0.5 - a.0, py);
            let value = ((reach - distance).clamp(0.0, 1.0) * 255.0) as u8;
            *slot = (*slot).max(value);
        }
    }
}

/// 1 本の線分。座標は始点を原点にしたもの。
struct Capsule {
    /// 終点の位置。
    dx: f64,
    dy: f64,
    /// 長さの 2 乗と長さ。
    squared: f64,
    length: f64,
}

impl Capsule {
    fn new(a: (f64, f64), b: (f64, f64)) -> Capsule {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let squared = dx * dx + dy * dy;
        Capsule {
            dx,
            dy,
            squared,
            length: squared.sqrt(),
        }
    }

    /// 点 `(px, py)` から線分までの距離。線分の上でいちばん近い点（端を越えれば端）までを測る。
    fn distance(&self, px: f64, py: f64) -> f64 {
        let t = if self.squared > 0.0 {
            ((px * self.dx + py * self.dy) / self.squared).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (ex, ey) = (px - self.dx * t, py - self.dy * t);
        (ex * ex + ey * ey).sqrt()
    }

    /// 高さ `py` の横線のうち、線分から `reach` 以内の部分（x の区間）。両端を中心とする円と、
    /// 線分に沿った長方形（線分への射影が線分の上にあり、線から `reach` 以内）の区間を合わせる。
    fn chord(&self, py: f64, reach: f64) -> Option<(f64, f64)> {
        // 負の距離や NaN には、近い点がない。
        if reach.is_nan() || reach < 0.0 {
            return None;
        }
        let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
        for (ex, ey) in [(0.0, 0.0), (self.dx, self.dy)] {
            let h = reach * reach - (py - ey) * (py - ey);
            if h >= 0.0 {
                let w = h.sqrt();
                (low, high) = (low.min(ex - w), high.max(ex + w));
            }
        }
        if self.squared > 0.0 {
            let mut band = (f64::NEG_INFINITY, f64::INFINITY);
            let (along, across) = (py * self.dy, py * self.dx);
            within(&mut band, self.dx, -along, self.squared - along);
            let side = reach * self.length;
            within(&mut band, self.dy, across - side, across + side);
            if band.0 <= band.1 {
                (low, high) = (low.min(band.0), high.max(band.1));
            }
        }
        (low <= high).then_some((low, high))
    }
}

/// 区間 `band` を、`k * x` が `from` 以上 `to` 以下になる x に狭める。
fn within(band: &mut (f64, f64), k: f64, from: f64, to: f64) {
    if k > 0.0 {
        *band = (band.0.max(from / k), band.1.min(to / k));
    } else if k < 0.0 {
        *band = (band.0.max(to / k), band.1.min(from / k));
    } else if !(from <= 0.0 && 0.0 <= to) {
        *band = (f64::INFINITY, f64::NEG_INFINITY);
    }
}

/// 中心が `low` と `high` の間にありうる画素の番号のうち、`start` から `count` 個の部分に
/// 入るもの。中心は番号 + 0.5 なので、`low` の切り捨てから `high` の切り上げの手前までで、
/// 両端に 0.5 画素以上の余裕がある（`low` と `high` の丸めはそれよりずっと小さい）。
fn span(low: f64, high: f64, start: i32, count: i32) -> std::ops::Range<i32> {
    // 座標が大きすぎても（無限大や NaN でも）部分の中に収めるので、範囲が空になるだけで済む。
    let (start, limit) = (start as i64, start as i64 + count.max(0) as i64);
    let first = (low.floor() as i64).clamp(start, limit);
    let end = (high.ceil() as i64).clamp(first, limit);
    first as i32..end as i32
}

/// 集めた覆いの分だけ、色を重ねる（ストレートアルファの source-over）。
fn blend(pixels: &mut [u8], coverage: &[u8], color: [u8; 3], opacity: f32) {
    for (pixel, &cover) in pixels.as_chunks_mut::<4>().0.iter_mut().zip(coverage) {
        if cover == 0 {
            continue;
        }
        let source = opacity * cover as f32 / 255.0;
        let behind = pixel[3] as f32 / 255.0;
        let alpha = source + behind * (1.0 - source);
        // The pixels are BGRA while the color is RGB.
        for (channel, value) in pixel[..3].iter_mut().zip(color.iter().rev()) {
            let mixed =
                (*value as f32 * source + *channel as f32 * behind * (1.0 - source)) / alpha;
            *channel = mixed.round().clamp(0.0, 255.0) as u8;
        }
        pixel[3] = (alpha * 255.0).round() as u8;
    }
}
