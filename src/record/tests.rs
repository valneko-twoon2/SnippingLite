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

#[test]
#[ignore = "Requires a desktop session with an audio output; plays a tone for a few seconds"]
fn records_system_audio_and_only_the_chosen_app() {
    let mut tone = play_tone(16);
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
            loopback: Some(audio::Loopback::Process(tone.id())),
            mic: false,
        },
        3,
        "app",
    );
    let (peak, seconds) = decode_audio(&app);
    assert!(peak > loud, "The app's own audio is silent: {peak}");
    assert!((seconds - 3.0).abs() < 0.6, "Audio length: {seconds}");
    // A silent process that does not belong to the tone's process tree.
    let mut silent = std::process::Command::new("ping")
        .args(["-n", "30", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let other = record_audio(
        Audio {
            loopback: Some(audio::Loopback::Process(silent.id())),
            mic: false,
        },
        2,
        "other",
    );
    let (peak, _) = decode_audio(&other);
    assert!(peak < 200, "Another process was recorded: {peak}");
    let _ = tone.kill();
    let _ = tone.wait();
    let _ = silent.kill();
    let _ = silent.wait();
    for path in [system, app, other] {
        let _ = std::fs::remove_file(path);
    }
}
