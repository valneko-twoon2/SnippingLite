//! The scale of the monitor under a point, which every window uses to size itself.

use super::*;
use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFO};

/// The nearest monitor's DPI, asked the long way round.
fn dpi_of_nearest(point: POINT) -> u32 {
    // SAFETY: plain monitor queries with valid out parameters.
    unsafe {
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let (mut x, mut y) = (0u32, 0u32);
        assert_eq!(
            GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y),
            0
        );
        x
    }
}

/// Inside the primary monitor, just outside each corner, and far away in every direction.
fn sample_points() -> Vec<POINT> {
    let mut points = vec![POINT { x: 0, y: 0 }];
    for far in [i32::MIN / 2, -100_000, 100_000, i32::MAX / 2] {
        points.push(POINT { x: far, y: 0 });
        points.push(POINT { x: 0, y: far });
        points.push(POINT { x: far, y: far });
    }
    let r = monitor_rect(POINT { x: 0, y: 0 });
    for (x, y) in [
        (r.left, r.top),
        (r.right - 1, r.bottom - 1),
        (r.right, r.bottom),
        (r.left - 1, r.top - 1),
    ] {
        points.push(POINT { x, y });
    }
    points
}

/// The whole area of the monitor Windows picks for `point`.
fn monitor_rect(point: POINT) -> RECT {
    // SAFETY: plain monitor queries with valid out parameters.
    unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        assert_ne!(GetMonitorInfoW(monitor, &mut info), 0);
        info.rcMonitor
    }
}

#[test]
fn the_scale_is_the_dpi_of_the_nearest_monitor_over_96() {
    let points = sample_points();
    // SAFETY: plain monitor queries with valid out parameters.
    unsafe {
        for point in points {
            let scale = scale_at(point);
            assert_eq!(
                scale,
                dpi_of_nearest(point) as f32 / 96.0,
                "{},{}",
                point.x,
                point.y
            );
            assert!(
                (1.0..=6.0).contains(&scale),
                "{scale} at {},{}",
                point.x,
                point.y
            );
        }
    }
}

#[test]
fn the_monitor_list_picks_the_same_monitor_and_dpi_as_windows() {
    // 窓を置く規則（ui::monitors）はモニターの一覧から計算する。本物の一覧では、Windows の
    // MonitorFromPoint と同じモニターと DPI を選び、一覧を囲む矩形は仮想画面になる。
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };
    // SAFETY: plain monitor queries.
    let list = unsafe { monitors::current() };
    assert!(!list.is_empty(), "at least one monitor");
    let edges = |r: RECT| (r.left, r.top, r.right, r.bottom);
    for point in sample_points() {
        let chosen = monitors::nearest(&list, point).expect("a monitor");
        assert_eq!(
            edges(chosen.rect),
            edges(monitor_rect(point)),
            "{},{}",
            point.x,
            point.y
        );
        assert_eq!(chosen.dpi, dpi_of_nearest(point), "{},{}", point.x, point.y);
        assert!(monitors::covered(&list, chosen.rect));
    }
    let union = list.iter().fold(list[0].rect, |u, m| RECT {
        left: u.left.min(m.rect.left),
        top: u.top.min(m.rect.top),
        right: u.right.max(m.rect.right),
        bottom: u.bottom.max(m.rect.bottom),
    });
    // SAFETY: plain metric queries.
    let screen = unsafe {
        let (x, y) = (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        );
        (
            x,
            y,
            x + GetSystemMetrics(SM_CXVIRTUALSCREEN),
            y + GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    assert_eq!(edges(union), screen, "the virtual screen");
    // 作業領域はそのモニターの中にある。
    for m in &list {
        assert!(m.work.left >= m.rect.left && m.work.right <= m.rect.right);
        assert!(m.work.top >= m.rect.top && m.work.bottom <= m.rect.bottom);
    }
}

#[test]
fn common_dpi_settings_scale_whole_pixels_without_gaps_or_overlap() {
    // Windows offers 100% to 300% in steps; neighbouring parts laid out in DIPs must still
    // have room after rounding, so that no hit area is lost between two buttons.
    for percent in (100..=300).step_by(25) {
        let scale = percent as f32 / 100.0;
        let edges: Vec<i32> = (0..=20)
            .map(|dip| (dip as f32 * 24.0 * scale).round() as i32)
            .collect();
        for pair in edges.windows(2) {
            assert!(pair[1] > pair[0], "{percent}%: empty cell {pair:?}");
        }
        assert_eq!(edges[20], (480.0 * scale).round() as i32, "{percent}%");
    }
}
