use super::*;
use crate::record::audio::RATE;
use windows::core::HSTRING;

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

/// 既定の音声の出力（スピーカー）があるか。GitHub Actions のランナーのように、音声の機器がない
/// 環境では、システムの音を録れないので、このテストは確かめようがない。
fn audio_output_available() -> bool {
    use windows::Win32::{
        Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator},
        System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
    };
    // SAFETY: plain COM calls on this thread.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .and_then(|devices| devices.GetDefaultAudioEndpoint(eRender, eConsole))
            .is_ok()
    }
}

#[test]
#[ignore = "Requires a desktop session with an audio output; plays a tone for a few seconds"]
fn records_system_audio_and_only_the_chosen_app() {
    if !audio_output_available() {
        eprintln!("skipped: this machine has no audio output");
        return;
    }
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
