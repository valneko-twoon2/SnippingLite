use super::*;

#[test]
#[ignore = "Requires an unlocked Windows desktop and Windows Graphics Capture"]
fn records_covered_window_without_other_windows() {
    use crate::window_capture::test_support::{center, CoveredWindow};
    let fixture = CoveredWindow::new();
    assert!(fixture.desktop_is_red());
    let path = std::env::temp_dir().join(format!(
        "snippinglite-window-test-{}.mp4",
        std::process::id()
    ));
    let options = Options {
        fps: 60,
        cursor: false,
        window: Some(Target::new(fixture.target, true)),
        audio: Audio::default(),
    };
    let recording = start(
        RECT {
            left: 80,
            top: 80,
            right: 400,
            bottom: 320,
        },
        path.clone(),
        Vec::new(),
        options,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while recording.started.get().is_none() && !recording.failed() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(300));
    let (_, frame) = recording
        .stop()
        .expect("Record the fully covered target window");
    assert!(frame.warning.is_none(), "{:?}", frame.warning);
    let pixels = center(&frame.pixels, frame.width, frame.height);
    assert!(
        u16::from(pixels[1]) > u16::from(pixels[0]) + 60
            && u16::from(pixels[1]) > u16::from(pixels[2]) + 60,
        "A different window was included in the video: {pixels:?}"
    );
    let data = std::fs::read(&path).unwrap();
    if let Some(folder) = std::env::var_os("SNIPPINGLITE_RECORDING_TEST_OUTPUT") {
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(PathBuf::from(folder).join("covered-window.mp4"), &data).unwrap();
    }
    std::fs::remove_file(path).unwrap();
    assert_eq!(&data[4..8], b"ftyp");
    assert!(fixture.desktop_is_red());
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Media Foundation encoder"]
fn recording_throughput() {
    // SAFETY: the benchmark owns its capture DC and bitmap on this thread.
    unsafe {
        let mut grabber = Grabber::new(
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            &[],
            true,
        )
        .unwrap();
        let begin = Instant::now();
        let mut frames = 0;
        while begin.elapsed() < Duration::from_secs(2) {
            grabber.grab();
            frames += 1;
        }
        eprintln!(
            "1080p capture-only capacity: {:.1} frames/s",
            f64::from(frames) / begin.elapsed().as_secs_f64()
        );
    }
    for fps in crate::config::FRAME_RATES {
        let path = std::env::temp_dir().join(format!(
            "snippinglite-throughput-{}-{fps}.mp4",
            std::process::id()
        ));
        let recording = start(
            RECT {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            path.clone(),
            Vec::new(),
            Options {
                fps,
                cursor: true,
                window: None,
                audio: Audio::default(),
            },
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while recording.started.get().is_none() && !recording.failed() && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_secs(2));
        let elapsed = recording.elapsed();
        let (_, frame) = recording.stop().unwrap();
        std::fs::remove_file(path).unwrap();
        let count = f64::from(frame.captured_frames.saturating_sub(1));
        eprintln!(
            "1080p / {fps} fps target: {:.1} fresh frames/s; capture {:.2}ms; encode {:.2}ms",
            count / elapsed.as_secs_f64(),
            frame.capture_time.as_secs_f64() * 1000.0 / count,
            frame.encode_time.as_secs_f64() * 1000.0 / count
        );
        assert!(frame.warning.is_none(), "{:?}", frame.warning);
    }
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Media Foundation encoder"]
fn records_rectangular_and_freeform_mp4() {
    for freeform in [false, true] {
        let options = Options {
            fps: if freeform { 60 } else { 30 },
            cursor: freeform,
            window: None,
            audio: Audio::default(),
        };
        let path = std::env::temp_dir().join(format!(
            "snippinglite-recording-test-{}-{freeform}.mp4",
            std::process::id()
        ));
        let points = if freeform {
            [
                (0, 0),
                (641, 0),
                (641, 240),
                (320, 240),
                (320, 481),
                (0, 481),
            ]
            .map(|(x, y)| POINT { x, y })
            .to_vec()
        } else {
            Vec::new()
        };
        let recording = start(
            RECT {
                left: 0,
                top: 0,
                right: 641,
                bottom: 481,
            },
            path.clone(),
            points,
            options,
        )
        .expect("Start the recording worker");
        let deadline = Instant::now() + Duration::from_secs(15);
        while recording.started.get().is_none() && !recording.failed() && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(300));
        let elapsed = recording.elapsed();
        let (_, frame) = recording.stop().expect("Finalize the MP4 recording");
        let bytes = std::fs::read(&path).expect("Read the generated MP4");
        if let Some(folder) = std::env::var_os("SNIPPINGLITE_RECORDING_TEST_OUTPUT") {
            let folder = PathBuf::from(folder);
            std::fs::create_dir_all(&folder).expect("Create the recording diagnostics folder");
            std::fs::write(
                folder.join(if freeform {
                    "freeform.mp4"
                } else {
                    "rectangle.mp4"
                }),
                &bytes,
            )
            .expect("Save the diagnostic recording");
        }
        std::fs::remove_file(&path).expect("Remove the temporary recording");
        assert_eq!((frame.width, frame.height), (640, 480));
        assert!(frame.warning.is_none(), "{:?}", frame.warning);
        assert_eq!(&bytes[4..8], b"ftyp");
        assert!(bytes.windows(4).any(|atom| atom == b"moov"));
        assert!(bytes.windows(4).any(|atom| atom == b"mdat"));
        let sizes = bytes
            .windows(4)
            .rposition(|atom| atom == b"stsz")
            .expect("MP4 sample sizes");
        let samples = u32::from_be_bytes(bytes[sizes + 12..sizes + 16].try_into().unwrap());
        assert!(samples > 1, "The MP4 must contain multiple video frames");
        let maximum = (elapsed.as_secs_f64() * f64::from(options.fps)).ceil() as u32 + 3;
        assert!(
            samples <= maximum,
            "Encoder warm-up added repeated frames: {samples} frames for {elapsed:?} of recording"
        );
        let media = bytes
            .windows(4)
            .rposition(|atom| atom == b"mdhd")
            .expect("MP4 media header");
        let scale_offset = media + if bytes[media + 4] == 1 { 24 } else { 16 };
        let timescale =
            u32::from_be_bytes(bytes[scale_offset..scale_offset + 4].try_into().unwrap());
        let timing = bytes
            .windows(4)
            .rposition(|atom| atom == b"stts")
            .expect("MP4 sample timing");
        let first_delta = u32::from_be_bytes(bytes[timing + 16..timing + 20].try_into().unwrap());
        assert!(
            u64::from(first_delta) * u64::from(options.fps) <= 2 * u64::from(timescale),
            "The first frame lasts {:.3}s; encoder initialization must not freeze it",
            f64::from(first_delta) / f64::from(timescale)
        );
        if freeform {
            let outside = (300 * frame.width as usize + 400) * 4;
            assert_eq!(&frame.pixels[outside..outside + 4], &[0, 0, 0, 255]);
        }
    }
}

fn area(width: i32, height: i32) -> RECT {
    RECT {
        left: 10,
        top: 10,
        right: 10 + width,
        bottom: 10 + height,
    }
}

fn options(fps: u32) -> Options {
    Options {
        fps,
        cursor: false,
        window: None,
        audio: Audio::default(),
    }
}

#[test]
fn start_rejects_bad_settings_without_creating_a_file() {
    let path =
        std::env::temp_dir().join(format!("snippinglite-rejected-{}.mp4", std::process::id()));
    let error = |area, options| match start(area, path.clone(), Vec::new(), options) {
        Ok(recording) => {
            let _ = recording.stop();
            panic!("The recording should not start")
        }
        Err(error) => error,
    };
    for fps in [0, 1, 29, 31, 59, 61, 120, u32::MAX] {
        assert_eq!(
            error(area(320, 240), options(fps)),
            tr("Unsupported recording frame rate."),
            "{fps} fps"
        );
    }
    // Sizes are rounded down to even numbers and must reach MIN_SIZE (34): 33 becomes 32.
    for (width, height) in [
        (0, 0),
        (32, 240),
        (320, 32),
        (33, 240),
        (320, 33),
        (34, 33),
        (33, 35),
        (-40, 240),
        (320, -40),
    ] {
        assert_eq!(
            error(area(width, height), options(30)),
            tr("The selected area is too small to record."),
            "{width}x{height}"
        );
    }
    // A window that does not exist (anymore) is refused instead of recording the desktop.
    let gone = Options {
        window: Some(Target::new(null_mut(), false)),
        ..options(30)
    };
    assert_eq!(
        error(area(320, 240), gone),
        tr("Could not capture the selected window.")
    );
    assert!(!path.exists());
}

#[test]
#[ignore = "Requires an unlocked Windows desktop and Media Foundation encoder"]
fn records_the_smallest_areas_at_even_sizes() {
    for ((width, height), expected) in [
        ((34, 34), (34, 34)),
        ((35, 35), (34, 34)),
        ((35, 67), (34, 66)),
        ((34, 240), (34, 240)),
    ] {
        let path = std::env::temp_dir().join(format!(
            "snippinglite-small-{width}x{height}-{}.mp4",
            std::process::id()
        ));
        let recording = start(area(width, height), path.clone(), Vec::new(), options(30)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while recording.started.get().is_none() && !recording.failed() && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(200));
        let (_, frame) = recording.stop().expect("Record a small area");
        assert_eq!((frame.width, frame.height), expected);
        assert_eq!(&std::fs::read(&path).unwrap()[4..8], b"ftyp");
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn recording_mask_preserves_concave_selection() {
    let polygon = [(0, 0), (4, 0), (4, 2), (2, 2), (2, 4), (0, 4)].map(|(x, y)| POINT { x, y });
    let mut pixels = vec![255; 4 * 4 * 4];
    for run in outside_runs(4, 4, &polygon) {
        pixels[run].fill(0);
    }
    for y in 0..4 {
        for x in 0..4 {
            let i = (y * 4 + x) * 4;
            assert_eq!(pixels[i], if y >= 2 && x >= 2 { 0 } else { 255 });
        }
    }
}

#[test]
fn recording_mask_clips_to_encoder_dimensions() {
    let polygon = [(-2, -1), (2, -1), (2, 3), (-2, 3)].map(|(x, y)| POINT { x, y });
    assert_eq!(outside_runs(4, 2, &polygon), vec![8..16, 24..32]);
    assert!(outside_runs(4, 2, &[]).is_empty());
    let rectangle = [(0, 0), (5, 0), (5, 3), (0, 3)].map(|(x, y)| POINT { x, y });
    assert!(outside_runs(4, 2, &rectangle).is_empty());
}

/// A 1 kHz tone played by a child process, so the sound belongs to a process
/// other than the test.
fn play_tone(seconds: u32) -> std::process::Child {
    let path = std::env::temp_dir().join(format!("snippinglite-tone-{}.wav", std::process::id()));
    let rate = 44_100u32;
    let samples: Vec<u8> = (0..rate * seconds)
        .flat_map(|i| {
            let phase = i as f32 * 1000.0 * std::f32::consts::TAU / rate as f32;
            ((phase.sin() * 12000.0) as i16).to_le_bytes()
        })
        .collect();
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + samples.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend(rate.to_le_bytes());
    wav.extend((rate * 2).to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((samples.len() as u32).to_le_bytes());
    wav.extend(samples);
    std::fs::write(&path, wav).unwrap();
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "(New-Object Media.SoundPlayer '{}').PlaySync()",
                path.display()
            ),
        ])
        .spawn()
        .unwrap()
}

/// Records a small area for `seconds` with the given audio and returns the file.
fn record_audio(audio: Audio, seconds: u64, name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("snippinglite-{name}-{}.mp4", std::process::id()));
    let recording = start(
        RECT {
            left: 0,
            top: 0,
            right: 320,
            bottom: 240,
        },
        path.clone(),
        Vec::new(),
        Options {
            fps: 30,
            cursor: false,
            window: None,
            audio,
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while recording.started.get().is_none() && !recording.failed() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_secs(seconds));
    recording.stop().expect("Finish the recording");
    path
}

/// Decodes the audio track of `path`: its loudest sample and its length in seconds.
fn decode_audio(path: &Path) -> (i16, f64) {
    const FIRST_AUDIO_STREAM: u32 = 0xFFFF_FFFD;
    const END_OF_STREAM: u32 = 2;
    // SAFETY: plain Media Foundation calls; the locked buffer is only read while locked.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL).unwrap();
        let reader =
            MFCreateSourceReaderFromURL(&HSTRING::from(path.to_string_lossy().as_ref()), None)
                .unwrap();
        reader
            .SetCurrentMediaType(FIRST_AUDIO_STREAM, None, &audio_type(false).unwrap())
            .expect("The file has an audio track");
        let (mut peak, mut bytes) = (0i16, 0usize);
        loop {
            let (mut flags, mut sample) = (0u32, None);
            reader
                .ReadSample(
                    FIRST_AUDIO_STREAM,
                    0,
                    None,
                    Some(&mut flags),
                    None,
                    Some(&mut sample),
                )
                .unwrap();
            if flags & END_OF_STREAM != 0 {
                break;
            }
            let Some(sample) = sample else { continue };
            let buffer = sample.ConvertToContiguousBuffer().unwrap();
            let (mut data, mut length) = (null_mut(), 0u32);
            buffer.Lock(&mut data, None, Some(&mut length)).unwrap();
            let pcm = std::slice::from_raw_parts(data, length as usize);
            for pair in pcm.as_chunks::<2>().0 {
                peak = peak.max(i16::from_le_bytes(*pair).saturating_abs());
            }
            bytes += length as usize;
            buffer.Unlock().unwrap();
        }
        let _ = MFShutdown();
        (peak, bytes as f64 / f64::from(RATE * 4))
    }
}

/// Ends a helper process when the test ends, also when an assertion fails: a process left
/// running would keep inherited handles open, such as the coverage data file.
struct Helper(std::process::Child);

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore = "Requires a desktop session with an audio output; plays a tone for a few seconds"]
fn records_system_audio_and_only_the_chosen_app() {
    let tone = Helper(play_tone(16));
    std::thread::sleep(Duration::from_millis(2000));
    let loud = 3000;
    let system = record_audio(
        Audio {
            loopback: Some(audio::Loopback::System),
            mic: false,
        },
        3,
        "system",
    );
    let (peak, seconds) = decode_audio(&system);
    assert!(peak > loud, "The system audio is silent: {peak}");
    assert!((seconds - 3.0).abs() < 0.6, "Audio length: {seconds}");
    let app = record_audio(
        Audio {
            loopback: Some(audio::Loopback::Process(tone.0.id())),
            mic: false,
        },
        3,
        "app",
    );
    let (peak, seconds) = decode_audio(&app);
    assert!(peak > loud, "The app's own audio is silent: {peak}");
    assert!((seconds - 3.0).abs() < 0.6, "Audio length: {seconds}");
    // A silent process that does not belong to the tone's process tree.
    let silent = Helper(
        std::process::Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let other = record_audio(
        Audio {
            loopback: Some(audio::Loopback::Process(silent.0.id())),
            mic: false,
        },
        2,
        "other",
    );
    let (peak, _) = decode_audio(&other);
    assert!(peak < 200, "Another process was recorded: {peak}");
    for path in [system, app, other] {
        let _ = std::fs::remove_file(path);
    }
}

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
