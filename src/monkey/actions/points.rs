//! 押す点と動かす道筋の生成。

use super::*;

/// 押したまま動かす先: 前の点の近く。ときどき画面の外まで（マウスをつかんでいる間は届く）。
pub(super) fn drift(rng: &mut Rng, now: &Snapshot, (x, y): P) -> P {
    let reach = if rng.chance(0.2) { 400 } else { 60 };
    (
        (x + rng.range(-reach..=reach)).clamp(-20, now.width + 20),
        (y + rng.range(-reach..=reach)).clamp(-20, now.height + 20),
    )
}

/// 押す点。ウィンドウを選ぶときは的の中（ほかのアプリの窓を撮らない）、書き込み中はたいてい
/// 切り取った範囲の中、そのほかは画面のどこか。ツールバーや書き込みのバーの下の点は、そちらへ
/// 届くので避ける。
pub(super) fn spot(rng: &mut Rng, now: &Snapshot, scene: &Scene) -> P {
    let mut point = (0, 0);
    for _ in 0..8 {
        point = if now.mode == MODE_WINDOW && now.session.is_none() {
            let r = *rng.pick(&scene.targets.rects);
            (
                rng.range(r.left..r.right) - now.origin.x,
                rng.range(r.top..r.bottom) - now.origin.y,
            )
        } else if let Some(look) = now.session.filter(|_| rng.chance(0.75)) {
            let a = look.area;
            (rng.range(a.left..a.right), rng.range(a.top..a.bottom))
        } else {
            (
                rng.range(0..now.width.max(1)),
                rng.range(0..now.height.max(1)),
            )
        };
        let screen = (point.0 + now.origin.x, point.1 + now.origin.y);
        let inside = |r: &Option<RECT>| {
            r.is_some_and(|r| {
                screen.0 >= r.left && screen.0 < r.right && screen.1 >= r.top && screen.1 < r.bottom
            })
        };
        if !inside(&now.toolbar) && !inside(&now.bar_rect) {
            break;
        }
    }
    point
}

/// 押してから動かす道筋。範囲は小さいもの（録画の下限 34 のまわり、幅 0 を含む）が多く、
/// ときどき大きくする。自由形式の切り取りのために点を 0〜8 個はさむ。
pub(super) fn path(rng: &mut Rng, now: &Snapshot, scene: &Scene) -> Vec<P> {
    let first = spot(rng, now, scene);
    let reach = match rng.below(10) {
        0..=2 => 40,
        3..=8 => 250,
        _ => 700,
    };
    let mut points = vec![first];
    for _ in 0..rng.range(0..=8) {
        let (x, y) = points[points.len() - 1];
        let step = reach / 3 + 1;
        points.push((
            (x + rng.range(-step..=step)).clamp(0, now.width - 1),
            (y + rng.range(-step..=step)).clamp(0, now.height - 1),
        ));
    }
    let end = (
        (first.0 + rng.range(-reach..=reach)).clamp(0, now.width - 1),
        (first.1 + rng.range(-reach..=reach)).clamp(0, now.height - 1),
    );
    points.push(if rng.chance(0.05) { first } else { end });
    points
}

/// 押すボタンと、離す別のボタン（`count` 個のボタンの番号）。同じボタンなら、ずらしではなく
/// クリックになる。
pub(super) fn two_buttons(rng: &mut Rng, count: i32) -> (i32, i32) {
    let from = rng.range(0..count);
    let to = (from + rng.range(1..count)) % count;
    (from, to)
}

/// ツールバーを動かす先（左上、画面の座標）。今いるモニターの中。
pub(super) fn toolbar_spot(rng: &mut Rng, now: &Snapshot) -> P {
    let bounds = now.toolbar.unwrap_or(now.toolbar_rect);
    // SAFETY: plain monitor queries.
    let monitor = unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let corner = POINT {
            x: bounds.left,
            y: bounds.top,
        };
        GetMonitorInfoW(
            MonitorFromPoint(corner, MONITOR_DEFAULTTONEAREST),
            &mut info,
        );
        info.rcMonitor
    };
    let (width, height) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
    (
        rng.range(monitor.left..=(monitor.right - width).max(monitor.left)),
        rng.range(monitor.top..=(monitor.bottom - height).max(monitor.top)),
    )
}
