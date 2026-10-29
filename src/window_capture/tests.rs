use super::*;
use test_support::*;
use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};

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
            window: Some(target),
            audio: crate::record::Audio::default()
        }
    )
    .is_err());
    assert!(!path.exists());
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Windows Graphics Capture"]
fn capture_ignores_occluding_windows_and_keeps_updating() {
    let fixture = CoveredWindow::new();
    assert_eq!(
        Target::new(fixture.target, false).process_id(),
        Some(std::process::id())
    );
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

fn bounds_tuple(r: RECT) -> (i32, i32, i32, i32) {
    (r.left, r.top, r.right, r.bottom)
}

#[test]
fn a_missing_window_has_no_bounds_and_no_process() {
    // Equivalence: a null handle and a destroyed window are both invalid targets.
    let null = Target::new(std::ptr::null_mut(), false);
    assert!(null.bounds().is_err());
    assert!(null.frame().is_err());
    assert_eq!(null.process_id(), None);
    assert!(WindowCapture::new(null, 100, 100, false).is_err());
    let fixture = CoveredWindow::new();
    let target = Target::new(fixture.target, false);
    assert!(target.bounds().is_ok());
    drop(fixture);
    assert!(target.bounds().is_err());
    assert!(snapshot(target).is_err());
}

#[test]
fn bounds_leave_out_the_translucent_border_and_the_non_client_area() {
    let fixture = CoveredWindow::new();
    let whole = Target::new(fixture.target, false);
    let frame = whole.frame().unwrap();
    let bounds = whole.bounds().unwrap();
    // A restored window loses one pixel on every side.
    assert_eq!(
        bounds_tuple(bounds),
        (
            frame.left + 1,
            frame.top + 1,
            frame.right - 1,
            frame.bottom - 1
        )
    );
    let client = Target::new(fixture.target, true).bounds().unwrap();
    assert!(client.left >= bounds.left && client.top >= bounds.top);
    assert!(client.right <= bounds.right && client.bottom <= bounds.bottom);
    assert!(
        client.bottom - client.top < bounds.bottom - bounds.top,
        "the title bar is not part of the client area"
    );
    // SAFETY: the window belongs to this test.
    unsafe {
        // A maximized window is already flush with the screen: no border is cut off.
        ShowWindow(fixture.target, SW_MAXIMIZE);
        let frame = whole.frame().unwrap();
        assert_eq!(bounds_tuple(whole.bounds().unwrap()), bounds_tuple(frame));
        // A minimized window has nothing to capture.
        ShowWindow(fixture.target, SW_MINIMIZE);
        assert!(whole.bounds().is_err());
        assert!(Target::new(fixture.target, true).bounds().is_err());
        // A window shrunk to nothing is not a valid capture area either.
        ShowWindow(fixture.target, SW_RESTORE);
        SetWindowPos(
            fixture.target,
            std::ptr::null_mut(),
            80,
            80,
            1,
            1,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let tiny = whole.bounds();
        assert!(tiny.is_err() || tiny.is_ok_and(|r| r.right > r.left && r.bottom > r.top));
    }
}

#[test]
fn the_sound_of_a_window_belongs_to_its_process() {
    let fixture = CoveredWindow::new();
    let target = Target::new(fixture.target, false);
    assert_eq!(target.process_id(), Some(std::process::id()));
    // A UWP frame window with only same-process children keeps the frame's own process.
    // SAFETY: the class and windows are created and destroyed here.
    unsafe {
        let class = crate::w("ApplicationFrameWindow");
        let spec = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: GetModuleHandleW(std::ptr::null()),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&spec);
        let frame = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            10,
            10,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        let child = CreateWindowExW(
            0,
            crate::w("STATIC").as_ptr(),
            class.as_ptr(),
            WS_CHILD,
            0,
            0,
            5,
            5,
            frame,
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        assert!(!frame.is_null() && !child.is_null());
        assert_eq!(
            Target::new(frame, false).process_id(),
            Some(std::process::id())
        );
        // The callback reports a child that lives in another process and ignores the frame's own.
        let mut found = (1u32, 0u32);
        let own = other_process(child, &mut found as *mut _ as LPARAM);
        assert_eq!((own, found), (0, (1, std::process::id())));
        let mut same = (std::process::id(), 0u32);
        assert_eq!(other_process(child, &mut same as *mut _ as LPARAM), 1);
        assert_eq!(same.1, 0);
        DestroyWindow(frame);
        UnregisterClassW(class.as_ptr(), GetModuleHandleW(std::ptr::null()));
    }
}

#[test]
fn the_apartment_copes_with_a_thread_that_is_already_single_threaded() {
    std::thread::spawn(|| {
        use windows_sys::Win32::System::Com::{
            CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED,
        };
        // SAFETY: balanced on this thread.
        unsafe {
            CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
            let apartment = Apartment::new().expect("An existing STA is usable");
            assert!(
                !apartment.0,
                "nothing to undo for an apartment this code did not start"
            );
            drop(apartment);
            CoUninitialize();
        }
        // A fresh thread starts the multithreaded apartment and releases it again.
        let apartment = Apartment::new().unwrap();
        assert!(apartment.0);
    })
    .join()
    .unwrap();
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Windows Graphics Capture"]
fn capture_rejects_empty_sizes() {
    let fixture = CoveredWindow::new();
    for (width, height) in [(0, 240), (320, 0), (-1, 240), (320, -1)] {
        assert!(
            WindowCapture::new(Target::new(fixture.target, false), width, height, false).is_err(),
            "{width}x{height}"
        );
    }
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Windows Graphics Capture"]
fn capture_follows_a_window_that_is_resized() {
    let fixture = CoveredWindow::new();
    let mut capture =
        WindowCapture::new(Target::new(fixture.target, false), 320, 240, true).unwrap();
    let length = capture.grab().unwrap().len();
    assert_eq!(length, 320 * 240 * 4);
    // SAFETY: the window belongs to this test.
    unsafe {
        SetWindowPos(
            fixture.target,
            std::ptr::null_mut(),
            80,
            80,
            200,
            150,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    fixture.recolor(BLUE);
    // The frame pool is recreated at the new size; the output keeps the encoder's size.
    let deadline = Instant::now() + Duration::from_secs(5);
    while capture.pool_size.Width > 250 {
        assert_eq!(capture.grab().unwrap().len(), length);
        assert!(
            Instant::now() < deadline,
            "The pool did not follow the new size"
        );
    }
    // Uncovered pixels are black, never a neighboring window.
    let pixels = capture.grab().unwrap();
    assert!(pixels[(239 * 320 + 319) * 4..][..3] == [0, 0, 0]);
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Windows Graphics Capture"]
fn capture_keeps_only_the_newest_of_several_waiting_frames() {
    let fixture = CoveredWindow::new();
    let mut capture =
        WindowCapture::new(Target::new(fixture.target, false), 320, 240, false).unwrap();
    capture.grab().unwrap();
    for color in [BLUE, RED, BLUE] {
        fixture.recolor(color);
    }
    std::thread::sleep(Duration::from_millis(300));
    let pixel = center(capture.grab().unwrap(), 320, 240);
    assert!(
        u16::from(pixel[1]) < u16::from(pixel[0]).max(u16::from(pixel[2])),
        "A newer frame replaced the green one: {pixel:?}"
    );
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Windows Graphics Capture"]
fn capture_stops_when_its_window_goes_away_or_is_minimized() {
    let fixture = CoveredWindow::new();
    let mut capture =
        WindowCapture::new(Target::new(fixture.target, false), 320, 240, false).unwrap();
    capture.grab().unwrap();
    // SAFETY: the window belongs to this test.
    unsafe { ShowWindow(fixture.target, SW_MINIMIZE) };
    assert!(
        capture.grab().is_err(),
        "a minimized window cannot be captured"
    );
    unsafe { ShowWindow(fixture.target, SW_RESTORE) };
    let mut capture =
        WindowCapture::new(Target::new(fixture.target, false), 320, 240, false).unwrap();
    capture.grab().unwrap();
    // The capture item closes with its window, which later grabs report.
    capture.closed.store(true, Ordering::Release);
    assert!(capture.grab().is_err());
    // Once the window is destroyed there is nothing left to grab.
    let mut capture =
        WindowCapture::new(Target::new(fixture.target, false), 320, 240, false).unwrap();
    capture.grab().unwrap();
    drop(fixture);
    assert!(capture.grab().is_err());
}
