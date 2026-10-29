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
