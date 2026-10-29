use super::*;

fn finished(result: Result<Frame, String>) -> Recording {
    // A recording whose thread has already ended, for the parts that do not need an encoder.
    Recording {
        stop: Arc::new(AtomicBool::new(false)),
        thread: std::thread::spawn(move || result),
        started: Arc::new(OnceLock::new()),
        path: PathBuf::from("finished.mp4"),
        area: area(100, 80),
        target: None,
    }
}

fn empty_frame() -> Frame {
    Frame {
        width: 100,
        height: 80,
        pixels: Vec::new(),
        warning: None,
        captured_frames: 0,
        capture_time: Duration::ZERO,
        encode_time: Duration::ZERO,
    }
}

#[test]
fn the_recording_folder_comes_from_the_settings_and_is_created() {
    let sandbox = crate::test_support::Sandbox::new();
    let chosen = sandbox.path().join("clips").join("nested");
    let settings = crate::config::Settings {
        video_folder: chosen.to_string_lossy().into_owned(),
        ..crate::config::parse_settings("")
    };
    crate::config::save_settings(&settings).unwrap();
    assert_eq!(video_folder().unwrap(), chosen);
    assert!(chosen.is_dir());
    // A file where the folder should be makes it impossible to create.
    let blocked = sandbox.path().join("blocked");
    std::fs::write(&blocked, b"x").unwrap();
    crate::config::save_settings(&crate::config::Settings {
        video_folder: blocked.join("inside").to_string_lossy().into_owned(),
        ..settings
    })
    .unwrap();
    assert!(video_folder().is_err());
}

#[test]
fn a_screen_recording_shows_its_own_area_and_a_lost_window_shows_none() {
    let recording = finished(Ok(empty_frame()));
    let shown = recording.display_area().unwrap();
    assert_eq!(
        (shown.left, shown.top, shown.right, shown.bottom),
        (10, 10, 110, 90)
    );
    // The target window no longer exists: there is no outline to draw.
    let lost = Recording {
        target: Some(Target::new(std::ptr::null_mut(), false)),
        ..finished(Ok(empty_frame()))
    };
    assert!(lost.display_area().is_none());
}

#[test]
fn a_window_recording_never_shows_more_than_the_recorded_size() {
    use crate::window_capture::test_support::CoveredWindow;
    let fixture = CoveredWindow::new();
    let target = Target::new(fixture.target, false);
    let bounds = target.bounds().unwrap();
    let window = |width: i32, height: i32| Recording {
        target: Some(target),
        area: area(width, height),
        ..finished(Ok(empty_frame()))
    };
    // Equivalence: the recorded size is larger than, equal to, or smaller than the window.
    let full = window(10_000, 10_000).display_area().unwrap();
    assert_eq!(
        (full.left, full.top, full.right, full.bottom),
        (bounds.left, bounds.top, bounds.right, bounds.bottom)
    );
    let small = window(100, 50).display_area().unwrap();
    assert_eq!(
        (small.right - small.left, small.bottom - small.top),
        (100, 50)
    );
    let exact = window(bounds.right - bounds.left, bounds.bottom - bounds.top)
        .display_area()
        .unwrap();
    assert_eq!((exact.right, exact.bottom), (bounds.right, bounds.bottom));
}

#[test]
fn the_clock_starts_with_the_first_frame_and_a_finished_thread_means_failure() {
    let recording = finished(Err("encoder gone".into()));
    assert_eq!(recording.elapsed(), Duration::ZERO);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !recording.failed() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(recording.failed());
    recording.started.set(Instant::now()).unwrap();
    std::thread::sleep(Duration::from_millis(5));
    assert!(recording.elapsed() >= Duration::from_millis(5));
    // The reason the recorder gave comes back from `stop`.
    assert_eq!(recording.stop().err().unwrap(), "encoder gone");
}

#[test]
fn stopping_returns_the_path_and_the_first_frame_or_reports_a_crash() {
    let (path, frame) = finished(Ok(empty_frame())).stop().unwrap();
    assert_eq!(path, PathBuf::from("finished.mp4"));
    assert_eq!((frame.width, frame.height), (100, 80));
    let crashed = Recording {
        thread: std::thread::spawn(|| -> Result<Frame, String> { panic!("recorder bug") }),
        ..finished(Ok(empty_frame()))
    };
    assert_eq!(crashed.stop().err().unwrap(), tr("The recorder crashed."));
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Media Foundation encoder"]
fn a_failed_encoder_leaves_no_file_behind() {
    let folder =
        std::env::temp_dir().join(format!("snippinglite-no-folder-{}", std::process::id()));
    let path = folder.join("missing").join("a.mp4");
    let stop = AtomicBool::new(true);
    let result = run(
        area(64, 64),
        &path,
        &stop,
        &OnceLock::new(),
        &[],
        options(60),
    );
    let error = result.err().expect("The folder does not exist");
    assert!(
        error.starts_with(tr("Could not start the video encoder")),
        "{error}"
    );
    assert!(!path.exists());
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Windows Graphics Capture"]
fn closing_the_recorded_window_keeps_what_was_recorded() {
    use crate::window_capture::test_support::CoveredWindow;
    let fixture = CoveredWindow::new();
    let path = std::env::temp_dir().join(format!("snippinglite-closed-{}.mp4", std::process::id()));
    let recording = start(
        area(320, 240),
        path.clone(),
        Vec::new(),
        Options {
            window: Some(Target::new(fixture.target, true)),
            ..options(30)
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while recording.started.get().is_none() && !recording.failed() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(500));
    drop(fixture);
    while !recording.failed() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        recording.failed(),
        "The recorder noticed the window closing"
    );
    let (_, frame) = recording.stop().expect("The recorded part is kept");
    assert!(frame.warning.is_some());
    assert_eq!(&std::fs::read(&path).unwrap()[4..8], b"ftyp");
    std::fs::remove_file(path).unwrap();
}
