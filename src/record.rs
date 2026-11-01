//! Screen recording to MP4: GDI frames go through the Media Foundation sink
//! writer, which encodes H.264 (hardware when available) and optional AAC audio.

use crate::{
    i18n::tr,
    window_capture::{Target, WindowCapture},
};
use std::{
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows::Win32::{
    Media::MediaFoundation::*,
    System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
};
use windows_sys::Win32::Foundation::{POINT, RECT};

mod audio;
mod screen;
mod writer;
use audio::AudioCapture;
pub use audio::{Audio, Loopback};
use screen::*;
use writer::*;

#[derive(Clone, Copy)]
pub struct Options {
    pub fps: u32,
    pub cursor: bool,
    pub window: Option<Target>,
    pub audio: Audio,
}
/// Hundreds of nanoseconds per second, the unit of Media Foundation times.
const TICKS: i64 = 10_000_000;
/// The smallest even width and height the Windows H.264 encoders accept; both
/// the hardware and the software encoder refuse 32 pixels or less.
pub const MIN_SIZE: i32 = 34;

/// The first frame of a finished recording, used as the notification preview.
pub struct Frame {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
    /// Set when the recording ended early but the file up to that point was kept.
    pub warning: Option<String>,
    /// 最後の名前に置けず、録画を失わないよう別の名前で残したときの、その名前。
    pub saved: Option<PathBuf>,
    #[cfg(test)]
    captured_frames: u32,
    #[cfg(test)]
    capture_time: Duration,
    #[cfg(test)]
    encode_time: Duration,
}

/// A recording in progress on its own thread.
pub struct Recording {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Result<Frame, String>>,
    started: Arc<OnceLock<Instant>>,
    pub path: PathBuf,
    area: RECT,
    target: Option<Target>,
}

/// The recording folder from the settings, created when missing.
/// 前に止まったプロセスが残した書きかけの録画は、ここで（次の録画を始めるときに）消す。
pub fn video_folder() -> std::io::Result<PathBuf> {
    let folder = crate::config::video_folder(&crate::config::load_settings());
    std::fs::create_dir_all(&folder)?;
    crate::staging::remove_leftovers(&folder);
    Ok(folder)
}

/// 保存しない録画を置く一時フォルダー。クリップボードから貼り付けられるよう 1 日は残し、
/// それより古い録画は次の録画を始めるときに消す。
pub fn temporary_folder() -> std::io::Result<PathBuf> {
    temporary_folder_in(&std::env::temp_dir(), std::time::SystemTime::now())
}

/// `base` の中の録画用の一時フォルダーを作り、`now` の時点で 1 日より古いファイルを消す。
/// テストが本物の一時フォルダーと時計に触れずに済むよう、場所と時刻を受け取る。
///
/// このフォルダー（`Snipping Lite\Recordings`）はこのアプリの録画専用なので、名前を問わず、
/// 1 日より古いファイルはすべて消す（書きかけで残った名前の違うファイルも消える）。中のフォルダーは
/// 消さずに残す。このアプリはフォルダーを作らないので、あれば誰かが置いたもので、中身ごと消すと
/// 取り返せない（`remove_file` はフォルダーを消さない）。
fn temporary_folder_in(base: &Path, now: std::time::SystemTime) -> std::io::Result<PathBuf> {
    let folder = base.join("Snipping Lite").join("Recordings");
    std::fs::create_dir_all(&folder)?;
    // 止まったプロセスの書きかけは、1 日を待たずに消す（貼り付けられるのは仕上がった録画だけ）。
    crate::staging::remove_leftovers(&folder);
    let day = std::time::Duration::from_secs(24 * 60 * 60);
    for entry in std::fs::read_dir(&folder)?.flatten() {
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|time| now.duration_since(time).ok())
            .is_some_and(|age| age > day);
        if stale {
            let _ = std::fs::remove_file(entry.path());
            crate::staging::step("old recording removed", &entry.path());
        }
    }
    Ok(folder)
}

/// Starts recording `area` (screen coordinates); the size is rounded down to
/// even numbers as H.264 requires. `points` is an optional freeform polygon
/// relative to the top-left of `area`. A window target overrides `area` with
/// its own bounds and never reads desktop pixels.
pub fn start(
    area: RECT,
    path: PathBuf,
    points: Vec<POINT>,
    options: Options,
) -> Result<Recording, String> {
    if !crate::config::FRAME_RATES.contains(&options.fps) {
        return Err(tr("Unsupported recording frame rate.").into());
    }
    let area = options
        .window
        .map_or(Ok(area), Target::bounds)
        .map_err(|_| tr("Could not capture the selected window.").to_string())?;
    let width = (area.right - area.left) & !1;
    let height = (area.bottom - area.top) & !1;
    if width < MIN_SIZE || height < MIN_SIZE {
        return Err(tr("The selected area is too small to record.").into());
    }
    let area = RECT {
        right: area.left + width,
        bottom: area.top + height,
        ..area
    };
    let stop = Arc::new(AtomicBool::new(false));
    // Publish the clock only after the encoder accepts the first frame.
    let started = Arc::new(OnceLock::new());
    let thread = {
        let (stop, path, started) = (stop.clone(), path.clone(), started.clone());
        std::thread::spawn(move || run(area, &path, &stop, &started, &points, options))
    };
    Ok(Recording {
        stop,
        thread,
        started,
        path,
        area,
        target: options.window,
    })
}

impl Recording {
    /// Current on-screen bounds for the recording outline, independent of pixels.
    pub fn display_area(&self) -> Option<RECT> {
        self.target.map_or(Some(self.area), |target| {
            target.bounds().ok().map(|mut rect| {
                rect.right = rect.right.min(rect.left + self.area.right - self.area.left);
                rect.bottom = rect.bottom.min(rect.top + self.area.bottom - self.area.top);
                rect
            })
        })
    }

    /// ウィンドウを録画している（窓を追いかけるので、画面の座標に縛られない）。
    pub fn follows_window(&self) -> bool {
        self.target.is_some()
    }

    /// Encoder warm-up is excluded from both the file timeline and the UI timer.
    pub fn elapsed(&self) -> Duration {
        self.started.get().map(Instant::elapsed).unwrap_or_default()
    }

    /// The recorder thread ended by itself, which only happens after a failure.
    pub fn failed(&self) -> bool {
        self.thread.is_finished()
    }

    /// Ends the recording, finishes the file and returns its first frame.
    pub fn stop(self) -> Result<(PathBuf, Frame), String> {
        self.stop.store(true, Ordering::Release);
        let frame = self
            .thread
            .join()
            .map_err(|_| tr("The recorder crashed.").to_string())??;
        Ok((frame.saved.clone().unwrap_or(self.path), frame))
    }
}

enum Source {
    Screen(Grabber),
    Window(WindowCapture),
}

impl Source {
    unsafe fn new(area: RECT, points: &[POINT], options: Options) -> Result<Self, String> {
        if let Some(target) = options.window {
            WindowCapture::new(
                target,
                area.right - area.left,
                area.bottom - area.top,
                options.cursor,
            )
            .map(Self::Window)
            .map_err(|_| tr("Could not capture the selected window.").to_string())
        } else {
            Grabber::new(area, points, options.cursor)
                .map(Self::Screen)
                .ok_or_else(|| tr("Could not prepare the screen capture.").to_string())
        }
    }

    unsafe fn grab(&mut self) -> Result<&[u8], String> {
        match self {
            Self::Screen(grabber) => Ok(grabber.grab()),
            Self::Window(capture) => capture
                .grab()
                .map_err(|_| tr("Could not capture the selected window.").to_string()),
        }
    }
}

fn run(
    area: RECT,
    path: &Path,
    stop: &AtomicBool,
    clock: &OnceLock<Instant>,
    points: &[POINT],
    options: Options,
) -> Result<Frame, String> {
    // SAFETY: COM and Media Foundation are initialized and shut down on this thread only.
    unsafe {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let started = MFStartup(MF_VERSION, MFSTARTUP_FULL).is_ok();
        // 一時名で書き、仕上げてから最後の名前に変える。録画の途中でプロセスが止まっても、
        // 保存先に再生できないファイルが最後の名前で残らない。
        let partial = crate::staging::partial(path);
        let result = record(area, &partial, stop, clock, points, options);
        if started {
            let _ = MFShutdown();
        }
        if com {
            CoUninitialize();
        }
        let mut frame = match result {
            Ok(frame) => frame,
            Err(error) => {
                let _ = std::fs::remove_file(&partial);
                return Err(error);
            }
        };
        // 書き終えた録画は作り直せないので、名前を変えられなくても消さない（`staging::keep`）。
        // どこにも置けなければ一時名のまま残し、場所をエラーで知らせる。書き終えているので、
        // アプリを終えた後の次の掃除（`staging::remove_leftovers`）も消さない。
        match crate::staging::keep(&partial, path) {
            Ok(saved) => {
                frame.saved = (saved != path).then_some(saved);
                Ok(frame)
            }
            Err(error) => Err(format!(
                "{}: {error}\n{}",
                tr("Could not finish the video file"),
                partial.display()
            )),
        }
    }
}

/// Writes the audio captured so far; sound is dropped for good if the encoder refuses it.
unsafe fn write_audio(
    audio: &mut Option<AudioCapture>,
    writer: &Writer,
    now: Instant,
    offset: i64,
    last: bool,
) {
    let (Some(capture), Some(stream)) = (audio.as_mut(), writer.audio) else {
        return;
    };
    let chunk = if last {
        capture.finish(now)
    } else {
        capture.read(now)
    };
    if let Some(chunk) = chunk {
        if write_to(
            writer,
            stream,
            &chunk.pcm,
            chunk.time + offset,
            chunk.duration,
        )
        .is_err()
        {
            *audio = None;
        }
    }
}

unsafe fn record(
    area: RECT,
    path: &Path,
    stop: &AtomicBool,
    clock: &OnceLock<Instant>,
    points: &[POINT],
    options: Options,
) -> Result<Frame, String> {
    let (width, height) = (area.right - area.left, area.bottom - area.top);
    let mut grabber = Source::new(area, points, options)?;
    let mut audio = options
        .audio
        .any()
        .then(|| AudioCapture::open(options.audio))
        .flatten();
    let writer = open(path, width, height, options.fps, audio.is_some()).map_err(|e| {
        format!(
            "{}: {}",
            tr("Could not start the video encoder"),
            e.message()
        )
    })?;
    if writer.audio.is_none() {
        audio = None;
    }
    // Capture only after opening the encoder so the first image is current.
    let mut first = grabber.grab()?.to_vec();
    first
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .for_each(|p| p[3] = 255);
    let duration = TICKS / options.fps as i64;
    let mut failure = write(&writer, &first, 0, duration)
        .err()
        .map(|e| format!("{}: {}", tr("Could not write a video frame"), e.message()));
    // A failure after the first frame still leaves a playable file to keep.
    let wrote_first = failure.is_none();
    let begin = Instant::now();
    if wrote_first {
        let _ = clock.set(begin);
    }
    if let Some(capture) = audio.as_mut() {
        capture.start(begin);
    }
    let frame = Duration::from_nanos(1_000_000_000 / options.fps as u64);
    let mut next = begin;
    #[cfg(test)]
    let (mut captured_frames, mut capture_time, mut encode_time) =
        (1, Duration::ZERO, Duration::ZERO);
    while failure.is_none() && !stop.load(Ordering::Acquire) {
        #[cfg(test)]
        let capture_begin = Instant::now();
        let pixels = match grabber.grab() {
            Ok(pixels) => pixels,
            Err(error) => {
                failure = Some(error);
                break;
            }
        };
        #[cfg(test)]
        {
            captured_frames += 1;
            capture_time += capture_begin.elapsed();
        }
        // Some encoders take a long time to open or accept the first sample.
        // Starting at one frame interval prevents filling that wait with copies
        // of the first image; later timestamps still follow real elapsed time.
        let time = duration + begin.elapsed().as_nanos() as i64 / 100;
        #[cfg(test)]
        let encode_begin = Instant::now();
        if let Err(error) = write(&writer, pixels, time, duration) {
            failure = Some(format!(
                "{}: {}",
                tr("Could not write a video frame"),
                error.message()
            ));
            break;
        }
        #[cfg(test)]
        {
            encode_time += encode_begin.elapsed();
        }
        // Video frames are stamped one frame late, so the sound is too.
        write_audio(&mut audio, &writer, Instant::now(), duration, false);
        next += frame;
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else if now - next > Duration::from_millis(250) {
            // Far behind: skip ahead instead of racing to catch up.
            next = now;
        }
    }
    drop(grabber);
    write_audio(&mut audio, &writer, Instant::now(), duration, true);
    drop(audio);
    let finalized = writer
        .sink
        .Finalize()
        .map_err(|e| format!("{}: {}", tr("Could not finish the video file"), e.message()));
    let warning = match (failure, finalized) {
        (Some(error), Ok(())) if wrote_first => Some(error),
        (Some(error), _) => return Err(error),
        (None, Err(error)) => return Err(error),
        (None, Ok(())) => None,
    };
    Ok(Frame {
        width,
        height,
        pixels: first,
        warning,
        saved: None,
        #[cfg(test)]
        captured_frames,
        #[cfg(test)]
        capture_time,
        #[cfg(test)]
        encode_time,
    })
}

#[cfg(test)]
mod tests;

/// Writes a short test video: `frames` solid frames of changing color at 30 fps, no sound.
#[cfg(test)]
pub(crate) fn write_sample(
    path: &Path,
    width: i32,
    height: i32,
    frames: u32,
) -> windows::core::Result<()> {
    // SAFETY: Media Foundation is started and shut down around the calls on this thread.
    unsafe {
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
        let result = (|| {
            let writer = open_with(path, width, height, 30, false)?;
            let step = TICKS / 30;
            for i in 0..frames {
                let shade = (i * 255 / frames.max(1)) as u8;
                let pixels = [shade, 255 - shade, 128, 255].repeat((width * height) as usize);
                write(&writer, &pixels, i64::from(i) * step, step)?;
            }
            writer.sink.Finalize()
        })();
        let _ = MFShutdown();
        result
    }
}
