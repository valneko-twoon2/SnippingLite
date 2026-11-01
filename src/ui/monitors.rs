//! モニターの並びと DPI。窓を置く規則を、本物のモニターの一覧（`current`）でも、テストが作った
//! 偽物の一覧でも同じように計算できるよう、一覧を引数で受け取る関数にしてある。座標はすべて
//! 物理画素（プロセスは Per-Monitor V2）。仮想画面の原点は主モニターの左上で、左や上にある
//! モニターは負の座標になる。

use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::{BOOL, LPARAM, POINT, RECT},
    Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW},
    UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
};

/// 1 台のモニター。`rect` は画面全体、`work` はタスクバーを除いた作業領域、`dpi` は実効 DPI、
/// `device` はモニターの名前（`\\.\DISPLAY1` などの UTF-16、残りは 0）。主モニターを入れ替えると
/// 同じ矩形に別のモニターが来るので、どのモニターかは名前で見分ける。
#[derive(Clone, Copy)]
pub struct Monitor {
    pub rect: RECT,
    pub work: RECT,
    pub dpi: u32,
    pub device: [u16; 32],
}

/// テストの偽物のモニターの名前を `Monitor::device` の形にする（32 文字より後は捨てる）。
#[cfg(test)]
pub fn device(name: &str) -> [u16; 32] {
    let mut out = [0u16; 32];
    for (slot, unit) in out.iter_mut().zip(name.encode_utf16()) {
        *slot = unit;
    }
    out
}

/// DPI を倍率にする（96 DPI が 100%）。
pub fn scale(dpi: u32) -> f32 {
    dpi as f32 / 96.0
}

/// WM_DPICHANGED の wParam が伝える新しい倍率（下位 16 ビットが横の DPI）。0 は 100% とみなす。
pub fn message_scale(wp: usize) -> f32 {
    match (wp & 0xffff) as u32 {
        0 => 1.0,
        dpi => scale(dpi),
    }
}

/// 今のモニターの一覧（EnumDisplayMonitors の順）。
pub unsafe fn current() -> Vec<Monitor> {
    unsafe extern "system" fn add(monitor: HMONITOR, _: HDC, _: *mut RECT, list: LPARAM) -> BOOL {
        let list = &mut *(list as *mut Vec<Monitor>);
        // MONITORINFOEXW を渡すと、モニターの名前（szDevice）も返る。
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(monitor, &mut info.monitorInfo) != 0 {
            let (mut x, mut y) = (96u32, 96u32);
            if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) != 0 {
                x = 96;
            }
            list.push(Monitor {
                rect: info.monitorInfo.rcMonitor,
                work: info.monitorInfo.rcWork,
                dpi: x,
                device: info.szDevice,
            });
        }
        1
    }
    let mut list: Vec<Monitor> = Vec::new();
    EnumDisplayMonitors(
        null_mut(),
        null(),
        Some(add),
        &mut list as *mut Vec<Monitor> as LPARAM,
    );
    list
}

fn edges(r: RECT) -> (i32, i32, i32, i32) {
    (r.left, r.top, r.right, r.bottom)
}

/// 並び（各モニターの名前、位置、解像度）が同じか。順番、DPI、作業領域は比べない。拡大率の変更は
/// 画素の位置を変えず、窓ごとの WM_DPICHANGED で扱う。名前も比べるのは、同じ解像度のモニターどうしで
/// 主モニターを入れ替えると、矩形の組は同じでも、同じ座標が別のモニターを指すようになるため。
pub fn same_layout(a: &[Monitor], b: &[Monitor]) -> bool {
    sorted(a.iter()) == sorted(b.iter())
}

/// モニターごとの（名前、矩形の辺）を並べ替えた一覧。
type Placed = ([u16; 32], (i32, i32, i32, i32));

fn sorted<'a>(monitors: impl Iterator<Item = &'a Monitor>) -> Vec<Placed> {
    let mut list: Vec<_> = monitors.map(|m| (m.device, edges(m.rect))).collect();
    list.sort_unstable();
    list
}

/// 2 つの矩形が重なる画素の数。右端と下端は矩形の外。
fn overlap(a: RECT, b: RECT) -> i64 {
    let width = i64::from(a.right.min(b.right)) - i64::from(a.left.max(b.left));
    let height = i64::from(a.bottom.min(b.bottom)) - i64::from(a.top.max(b.top));
    width.max(0) * height.max(0)
}

/// 画素の区間 `[low, high)` から `[from, to)` までの距離: 重なれば 0、すぐ隣の画素なら 1。
/// 隣のモニターの端の画素が、どちらのモニターからも 0 にならないように、画素の数で測る。
fn gap(low: i32, high: i32, from: i32, to: i32) -> i64 {
    (i64::from(from) - i64::from(high) + 1)
        .max(i64::from(low) - i64::from(to) + 1)
        .max(0)
}

/// 矩形から矩形までの距離の二乗。
fn distance(a: RECT, b: RECT) -> i64 {
    let dx = gap(a.left, a.right, b.left, b.right);
    let dy = gap(a.top, a.bottom, b.top, b.bottom);
    dx * dx + dy * dy
}

/// 点のあるモニター。どのモニターにもなければ、いちばん近いモニター（MonitorFromPoint の
/// MONITOR_DEFAULTTONEAREST と同じ選び方）。距離が同じなら一覧の先のもの。
pub fn nearest(monitors: &[Monitor], p: POINT) -> Option<&Monitor> {
    let pixel = RECT {
        left: p.x,
        top: p.y,
        right: p.x.saturating_add(1),
        bottom: p.y.saturating_add(1),
    };
    monitors.iter().min_by_key(|m| distance(m.rect, pixel))
}

/// 矩形といちばん広く重なるモニター。どれとも重ならなければ、矩形にいちばん近いモニター。
/// 窓の DPI も、Windows がこの選び方（いちばん広く重なるモニター）で決める。
pub fn most_of(monitors: &[Monitor], r: RECT) -> Option<&Monitor> {
    let widest = monitors
        .iter()
        .min_by_key(|m| std::cmp::Reverse(overlap(m.rect, r)))?;
    if overlap(widest.rect, r) > 0 {
        Some(widest)
    } else {
        monitors.iter().min_by_key(|m| distance(m.rect, r))
    }
}

/// 矩形のどの画素も、どれかのモニターの上にあるか。隣り合う 2 台にまたがっていてもよい。
/// 矩形をモニターの辺で升目に切ると、どの升目もモニターの中か外のどちらか一方になる。
pub fn covered(monitors: &[Monitor], r: RECT) -> bool {
    let mut xs = vec![r.left, r.right];
    let mut ys = vec![r.top, r.bottom];
    for m in monitors {
        xs.extend([m.rect.left, m.rect.right].map(|x| x.clamp(r.left, r.right)));
        ys.extend([m.rect.top, m.rect.bottom].map(|y| y.clamp(r.top, r.bottom)));
    }
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();
    xs.windows(2).all(|x| {
        ys.windows(2).all(|y| {
            monitors.iter().any(|m| {
                let cell = RECT {
                    left: x[0],
                    top: y[0],
                    right: x[1],
                    bottom: y[1],
                };
                overlap(m.rect, cell) > 0
            })
        })
    })
}

/// `r` を大きさを変えずに `bounds` の中へ動かす。入りきらなければ左端と上端をそろえる。
pub fn fit(r: RECT, bounds: RECT) -> RECT {
    let (width, height) = (r.right - r.left, r.bottom - r.top);
    let left = r.left.min(bounds.right - width).max(bounds.left);
    let top = r.top.min(bounds.bottom - height).max(bounds.top);
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// 画面からはみ出した窓を戻す先。どの画素もどれかのモニターの上にあれば、そのまま（利用者が
/// 置いた所を守る）。はみ出していれば（モニターを外した、解像度を下げた）、いちばん広く重なる
/// モニター（重ならなければいちばん近いモニター）の中へ寄せる。
pub fn keep_on_screen(monitors: &[Monitor], r: RECT) -> RECT {
    if covered(monitors, r) {
        return r;
    }
    most_of(monitors, r).map_or(r, |m| fit(r, m.rect))
}

/// モニターの上部中央に置く、幅 `width`、高さ `height` の窓。上端から 12 DIP 空ける
/// （README: ツールバーは上部中央に出る。録画中の表示も同じ所に出る）。
pub fn top_center(monitor: RECT, width: i32, height: i32, scale: f32) -> RECT {
    let left = (monitor.left + monitor.right - width) / 2;
    let top = monitor.top + (12.0 * scale).round() as i32;
    RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// 範囲 `area` にかかるモニターの並びが、`before` から `after` で変わったか（そのモニターの位置や
/// 解像度が変わった、外した、別のモニターがその場所に来た）。別のモニターかは名前で見分ける
/// （主モニターを入れ替えると、同じ矩形に別のモニターが来る）。範囲にかからないモニターの変化は
/// 数えない。範囲の座標が前と同じ画面を指しているかを確かめる。
pub fn moved_under(before: &[Monitor], after: &[Monitor], area: RECT) -> bool {
    let under = |list: &[Monitor]| sorted(list.iter().filter(|m| overlap(m.rect, area) > 0));
    under(before) != under(after)
}

#[cfg(test)]
mod tests;
