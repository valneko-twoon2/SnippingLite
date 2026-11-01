use super::*;

fn stops(x: i32, y: i32, scale: f32) -> bool {
    ui::contains(stop_button(scale), POINT { x, y })
}

#[test]
fn only_the_stop_button_ends_a_recording() {
    // The red dot and the elapsed time keep recording.
    assert!(!stops(20, 24, 1.0));
    assert!(!stops(60, 24, 1.0));
    // The button and its edges.
    assert!(stops(130, 24, 1.0));
    assert!(stops(100, 8, 1.0));
    assert!(stops(159, 39, 1.0));
    assert!(!stops(99, 24, 1.0));
    assert!(!stops(160, 24, 1.0));
    assert!(!stops(130, 7, 1.0));
    assert!(!stops(130, 40, 1.0));
    // The margin to the right of the button.
    assert!(!stops(164, 24, 1.0));
    // A scaled indicator moves the edges with it.
    assert!(stops(150, 12, 1.5));
    assert!(!stops(149, 24, 1.5));
    assert!(stops(239, 59, 1.5));
    assert!(!stops(240, 24, 1.5));
}

#[test]
fn the_indicator_sits_at_the_top_center_of_the_recorded_monitor_at_its_scale() {
    use crate::ui::monitors::Monitor;
    let monitor = |left, top, right, bottom, dpi| {
        let rect = RECT {
            left,
            top,
            right,
            bottom,
        };
        Monitor {
            rect,
            work: rect,
            dpi,
        }
    };
    // 主モニター A（100%）の上に C（2560x1440、200%、負の座標）。
    let list = [
        monitor(0, 0, 1920, 1080, 96),
        monitor(0, -1440, 2560, 0, 192),
    ];
    let at = |x, y, scale| {
        let r = indicator_rect(&list, POINT { x, y }, scale).unwrap();
        (r.left, r.top, r.right, r.bottom)
    };
    // 範囲の真ん中が C の上: 幅 336、高さ 96、(2560 - 336) / 2 = 1112、上端 -1440 から 24。
    assert_eq!(at(500, -700, 2.0), (1112, -1416, 1448, -1320));
    // A の上: 幅 168、(1920 - 168) / 2 = 876、上から 12。
    assert_eq!(at(300, 260, 1.0), (876, 12, 1044, 60));
    // 真ん中がどのモニターにもない（C だけが右へはみ出す所の下）: A まで 281、C まで 301。
    assert_eq!(at(2200, 300, 1.0), (876, 12, 1044, 60));
    assert!(indicator_rect(&[], POINT { x: 0, y: 0 }, 1.0).is_none());
}

#[test]
fn the_stop_button_fits_the_indicator_on_every_scale() {
    for step in 4..=16 {
        let scale = step as f32 / 4.0;
        let [l, t, r, b] = stop_button(scale);
        assert!(
            l > 92.0 * scale,
            "the button covers the time on scale {scale}"
        );
        assert!(r <= RECORD_WIDTH * scale && b <= RECORD_HEIGHT * scale && t >= 0.0);
        assert!(r - l >= 1.0 && b - t >= 1.0);
    }
}
