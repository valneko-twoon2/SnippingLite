use super::*;
use test_support::*;

#[test]
fn invalid_window_never_falls_back_to_the_desktop() {
    let target = Target::new(std::ptr::null_mut(), false);
    assert!(snapshot(target).is_err());
    let path = std::env::temp_dir().join(format!(
        "snippinglite-invalid-window-{}.mp4",
        std::process::id()
    ));
    assert!(crate::record::start(
        windows_sys::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: 320,
            bottom: 240
        },
        path.clone(),
        Vec::new(),
        crate::record::Options {
            fps: 60,
            cursor: false,
            window: Some(target)
        }
    )
    .is_err());
    assert!(!path.exists());
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Windows Graphics Capture"]
fn capture_ignores_occluding_windows_and_keeps_updating() {
    let fixture = CoveredWindow::new();
    assert!(
        fixture.desktop_is_red(),
        "The target must actually be completely covered"
    );
    let image = snapshot(Target::new(fixture.target, false)).expect("Capture the covered window");
    let pixel = center(&image.pixels, image.width, image.height);
    assert!(
        u16::from(pixel[1]) > u16::from(pixel[0]) + 60
            && u16::from(pixel[1]) > u16::from(pixel[2]) + 60,
        "Window pixels: {pixel:?}"
    );
    let client = snapshot(Target::new(fixture.target, true)).unwrap();
    let client_bounds = Target::new(fixture.target, true).bounds().unwrap();
    assert_eq!(
        (client.width, client.height),
        (
            client_bounds.right - client_bounds.left,
            client_bounds.bottom - client_bounds.top
        )
    );
    for (x, y) in [(3, 3), (client.width - 4, client.height - 4)] {
        let offset = ((y * client.width + x) * 4) as usize;
        assert!(
            client.pixels[offset + 1] > 150 && client.pixels[offset + 2] < 120,
            "The client-only crop includes non-client pixels"
        );
    }
    let mut capture =
        WindowCapture::new(Target::new(fixture.target, false), 320, 240, false).unwrap();
    capture.grab().unwrap();
    fixture.recolor(BLUE);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let pixel = center(capture.grab().unwrap(), 320, 240);
        if u16::from(pixel[0]) > u16::from(pixel[1]) + 60
            && u16::from(pixel[0]) > u16::from(pixel[2]) + 60
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Covered window did not update: {pixel:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        fixture.desktop_is_red(),
        "The covering window must remain above the target"
    );
}
