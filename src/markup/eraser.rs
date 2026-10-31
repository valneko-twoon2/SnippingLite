//! 消しゴム: なぞった道筋に触れた線を、線ごと消す。

use super::*;

/// 消しゴムが線に届く距離（DIP）。
pub(super) const ERASER_REACH: f32 = 8.0;

/// 消しゴムでなぞっている途中。
pub(super) struct Erasing {
    /// なぞり始める前の書き込み。線を消していれば、確定するときにこれを元に戻す一覧に積む。
    pub(super) before: Vec<Mark>,
    /// 前にマウスがあった所（画像の座標）。そこから今の所までの道筋で消す。
    pub(super) last: (f32, f32),
}

/// 画像の座標で、消しゴムが `from` から `to` へ動いた道筋（線分）が、線 `mark` から
/// 線の太さ + `reach` 以内を通るか。マウスから来る点だけで判定すると、速くなぞったときに点と点の
/// 間にある細い線を飛び越えるので、前の点から今の点までの道筋で判定する。押しただけのときは
/// `from` と `to` が同じ点になる。
pub(super) fn touches(mark: &Mark, from: (f32, f32), to: (f32, f32), reach: f32) -> bool {
    let limit = (mark.radius + reach).powi(2);
    let near = |a, b| apart(a, b, from, to) <= limit;
    match mark.points.as_slice() {
        [] => false,
        [only] => near(*only, *only),
        points => points.windows(2).any(|pair| near(pair[0], pair[1])),
    }
}

/// 点 `p` から線分 `a`–`b` までの距離の 2 乗。
fn to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length = dx * dx + dy * dy;
    let t = if length == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / length).clamp(0.0, 1.0)
    };
    (a.0 + dx * t - p.0).powi(2) + (a.1 + dy * t - p.1).powi(2)
}

/// 線分 `a`–`b` と `c`–`d` の距離の 2 乗。交われば 0。交わらなければ、4 つの端のどれかから
/// 相手の線分までの距離のうち、最も短いもの。
fn apart(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> f32 {
    // `p` から `q` へ向かって、`r` が左にあれば正、右にあれば負（外積）。
    let side = |p: (f32, f32), q: (f32, f32), r: (f32, f32)| {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    };
    // どちらの線分も、相手の両側に端があれば交わる。端が相手の線分の上にあるときは、
    // 下の端からの距離が 0 になる。
    if side(a, b, c) * side(a, b, d) < 0.0 && side(c, d, a) * side(c, d, b) < 0.0 {
        return 0.0;
    }
    to_segment(a, c, d)
        .min(to_segment(b, c, d))
        .min(to_segment(c, a, b))
        .min(to_segment(d, a, b))
}

/// 消しゴムが `to`（画像の座標）まで動いた。前にいた所からの道筋に触れた線を消す。
pub(super) unsafe fn erase(to: (f32, f32)) {
    let reach = ERASER_REACH * state().scale;
    let Some(session) = state().markup_session.as_mut() else {
        return;
    };
    let Some(erasing) = session.erasing.as_mut() else {
        return;
    };
    let from = std::mem::replace(&mut erasing.last, to);
    let before = session.marks.len();
    session.marks.retain(|mark| !touches(mark, from, to, reach));
    if session.marks.len() != before {
        render_all();
    }
}
