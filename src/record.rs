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
use windows::{
    core::HSTRING,
    Win32::{
        Media::MediaFoundation::*,
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
    },
};
use windows_sys::Win32::Foundation::{POINT, RECT};

mod audio;
mod screen;
pub use audio::{Audio, Loopback};
use audio::{AudioCapture, RATE};
use screen::*;

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
pub fn video_folder() -> std::io::Result<PathBuf> {
    let folder = crate::config::video_folder(&crate::config::load_settings());
    std::fs::create_dir_all(&folder)?;
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
        Ok((self.path, frame))
    }
}

struct Writer {
    sink: IMFSinkWriter,
    stream: u32,
    /// The AAC stream; `None` when the recording has no audio.
    audio: Option<u32>,
}

fn video_type(
    subtype: &windows::core::GUID,
    width: i32,
    height: i32,
    fps: u32,
) -> windows::core::Result<IMFMediaType> {
    // SAFETY: plain Media Foundation calls on objects created here.
    unsafe {
        let media = MFCreateMediaType()?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        media.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        media.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
        media.SetUINT64(&MF_MT_FRAME_RATE, ((fps as u64) << 32) | 1)?;
        media.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1 << 32) | 1)?;
        Ok(media)
    }
}

/// An audio stream needs the AAC encoder, which some Windows editions lack; the
/// recording then goes on without sound.
fn open(
    path: &Path,
    width: i32,
    height: i32,
    fps: u32,
    audio: bool,
) -> windows::core::Result<Writer> {
    match open_with(path, width, height, fps, audio) {
        Err(_) if audio => open_with(path, width, height, fps, false),
        result => result,
    }
}

/// 16-bit stereo audio at `RATE`: raw PCM, or AAC at 160 kbit/s.
fn audio_type(aac: bool) -> windows::core::Result<IMFMediaType> {
    // SAFETY: plain Media Foundation calls on objects created here.
    unsafe {
        let media = MFCreateMediaType()?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        media.SetGUID(
            &MF_MT_SUBTYPE,
            if aac {
                &MFAudioFormat_AAC
            } else {
                &MFAudioFormat_PCM
            },
        )?;
        media.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
        media.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE)?;
        media.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        if aac {
            media.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 20_000)?;
            media.SetUINT32(&MF_MT_AAC_PAYLOAD_TYPE, 0)?;
            media.SetUINT32(&MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION, 0x29)?;
        } else {
            media.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
            media.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, RATE * 4)?;
        }
        Ok(media)
    }
}

fn open_with(
    path: &Path,
    width: i32,
    height: i32,
    fps: u32,
    audio: bool,
) -> windows::core::Result<Writer> {
    // SAFETY: plain Media Foundation calls on objects created here.
    unsafe {
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 2)?;
        let attributes = attributes.ok_or_else(windows::core::Error::empty)?;
        attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
        // The capture loop already owns frame pacing; avoid a second wait here.
        attributes.SetUINT32(&MF_SINK_WRITER_DISABLE_THROTTLING, 1)?;
        let url = HSTRING::from(path.to_string_lossy().as_ref());
        let sink = MFCreateSinkWriterFromURL(&url, None, &attributes)?;
        let output = video_type(&MFVideoFormat_H264, width, height, fps)?;
        let bitrate = (width as f64 * height as f64 * fps as f64 * 0.1)
            .clamp(2e6, 20e6 * f64::from(fps) / 30.0) as u32;
        output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
        let stream = sink.AddStream(&output)?;
        let input = video_type(&MFVideoFormat_RGB32, width, height, fps)?;
        // A positive stride marks the rows as top-down, like the DIB section.
        input.SetUINT32(&MF_MT_DEFAULT_STRIDE, (width * 4) as u32)?;
        sink.SetInputMediaType(stream, &input, None)?;
        let audio = if audio {
            let audio_stream = sink.AddStream(&audio_type(true)?)?;
            sink.SetInputMediaType(audio_stream, &audio_type(false)?, None)?;
            Some(audio_stream)
        } else {
            None
        };
        sink.BeginWriting()?;
        Ok(Writer {
            sink,
            stream,
            audio,
        })
    }
}

fn write(writer: &Writer, pixels: &[u8], time: i64, duration: i64) -> windows::core::Result<()> {
    write_to(writer, writer.stream, pixels, time, duration)
}

fn write_to(
    writer: &Writer,
    stream: u32,
    data: &[u8],
    time: i64,
    duration: i64,
) -> windows::core::Result<()> {
    // SAFETY: the buffer is locked while it is filled and holds `data.len()` bytes.
    unsafe {
        let pixels = data;
        let buffer = MFCreateMemoryBuffer(pixels.len() as u32)?;
        let mut data = null_mut();
        buffer.Lock(&mut data, None, None)?;
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), data, pixels.len());
        buffer.Unlock()?;
        buffer.SetCurrentLength(pixels.len() as u32)?;
        let sample = MFCreateSample()?;
        sample.AddBuffer(&buffer)?;
        sample.SetSampleTime(time)?;
        sample.SetSampleDuration(duration)?;
        writer.sink.WriteSample(stream, &sample)
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
        let result = record(area, path, stop, clock, points, options);
        if started {
            let _ = MFShutdown();
        }
        if com {
            CoUninitialize();
        }
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
    }
}

/// Writes the audio captured so far; sound is dropped for good if the encoder refuses it.
unsafe fn write_audio(
    audio: &mut Option<AudioCapture>,
    writer: &Writer,
    now: Instant,
    offset: i64,
) {
    let (Some(capture), Some(stream)) = (audio.as_mut(), writer.audio) else {
        return;
    };
    if let Some(chunk) = capture.read(now) {
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
        write_audio(&mut audio, &writer, Instant::now(), duration);
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
    write_audio(&mut audio, &writer, Instant::now(), duration);
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
