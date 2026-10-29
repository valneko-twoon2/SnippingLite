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

#[test]
fn the_scale_is_the_dpi_of_the_nearest_monitor_over_96() {
    // Inside the primary monitor, just outside each corner, and far away in every direction.
    let mut points = vec![POINT { x: 0, y: 0 }];
    for far in [i32::MIN / 2, -100_000, 100_000, i32::MAX / 2] {
        points.push(POINT { x: far, y: 0 });
        points.push(POINT { x: 0, y: far });
        points.push(POINT { x: far, y: far });
    }
    // SAFETY: plain monitor queries with valid out parameters.
    unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let primary = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        assert_ne!(GetMonitorInfoW(primary, &mut info), 0);
        let r = info.rcMonitor;
        for (x, y) in [
            (r.left, r.top),
            (r.right - 1, r.bottom - 1),
            (r.right, r.bottom),
            (r.left - 1, r.top - 1),
        ] {
            points.push(POINT { x, y });
        }
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
