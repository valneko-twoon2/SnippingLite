//! Screen recording to MP4: GDI frames go through the Media Foundation sink
//! writer, which encodes H.264 (hardware when available). Only video is recorded.

use crate::i18n::tr;
use std::{
    ops::Range,
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
use windows_sys::Win32::{
    Foundation::{POINT, RECT},
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{
        DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, DI_NORMAL, HCURSOR,
        ICONINFO,
    },
};

#[derive(Clone, Copy)]
pub struct Options {
    pub fps: u32,
    pub cursor: bool,
}
/// Hundreds of nanoseconds per second, the unit of Media Foundation times.
const TICKS: i64 = 10_000_000;

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
}

/// The recording folder from the settings, created when missing.
pub fn video_folder() -> std::io::Result<PathBuf> {
    let folder = crate::config::video_folder(&crate::config::load_settings());
    std::fs::create_dir_all(&folder)?;
    Ok(folder)
}

/// Starts recording `area` (screen coordinates); the size is rounded down to
/// even numbers as H.264 requires. `points` is an optional freeform polygon
/// relative to the top-left of `area`.
pub fn start(
    area: RECT,
    path: PathBuf,
    points: Vec<POINT>,
    options: Options,
) -> Result<Recording, String> {
    if !crate::config::FRAME_RATES.contains(&options.fps) {
        return Err(tr("Unsupported recording frame rate.").into());
    }
    let width = (area.right - area.left) & !1;
    let height = (area.bottom - area.top) & !1;
    if width < 16 || height < 16 {
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
    })
}

impl Recording {
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

fn open(path: &Path, width: i32, height: i32, fps: u32) -> windows::core::Result<Writer> {
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
        sink.BeginWriting()?;
        Ok(Writer { sink, stream })
    }
}

fn write(writer: &Writer, pixels: &[u8], time: i64, duration: i64) -> windows::core::Result<()> {
    // SAFETY: the buffer is locked while it is filled and holds `pixels.len()` bytes.
    unsafe {
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
        writer.sink.WriteSample(writer.stream, &sample)
    }
}

/// Cursor hotspot metadata is cached until the cursor changes. GetIconInfo
/// allocates two GDI bitmaps, so calling it for every frame is unnecessary.
struct Cursor {
    icon: HCURSOR,
    hotspot: POINT,
}

impl Cursor {
    unsafe fn draw(&mut self, dc: HDC, area: &RECT) {
        let mut info: CURSORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
        if GetCursorInfo(&mut info) == 0 || info.flags & CURSOR_SHOWING == 0 {
            return;
        }
        if self.icon != info.hCursor {
            let mut icon: ICONINFO = std::mem::zeroed();
            if GetIconInfo(info.hCursor, &mut icon) == 0 {
                return;
            }
            self.icon = info.hCursor;
            self.hotspot = POINT {
                x: icon.xHotspot as i32,
                y: icon.yHotspot as i32,
            };
            if !icon.hbmMask.is_null() {
                DeleteObject(icon.hbmMask);
            }
            if !icon.hbmColor.is_null() {
                DeleteObject(icon.hbmColor);
            }
        }
        DrawIconEx(
            dc,
            info.ptScreenPos.x - self.hotspot.x - area.left,
            info.ptScreenPos.y - self.hotspot.y - area.top,
            info.hCursor,
            0,
            0,
            0,
            null_mut(),
            DI_NORMAL,
        );
    }
}

/// Precomputes byte ranges outside an even-odd polygon. MP4 has no alpha, so
/// each frame fills these ranges with black after drawing the cursor.
fn outside_runs(width: i32, height: i32, points: &[POINT]) -> Vec<Range<usize>> {
    let mut outside = Vec::new();
    if points.len() < 3 {
        return outside;
    }
    let mut intersections = Vec::new();
    for y in 0..height {
        intersections.clear();
        let scan = f64::from(y) + 0.5;
        let mut previous = points[points.len() - 1];
        for current in points {
            let (ay, by) = (f64::from(previous.y), f64::from(current.y));
            if (ay > scan) != (by > scan) {
                let x = f64::from(previous.x)
                    + (scan - ay) * (f64::from(current.x) - f64::from(previous.x)) / (by - ay);
                intersections.push(x);
            }
            previous = *current;
        }
        intersections.sort_by(f64::total_cmp);
        let row = (y as usize * width as usize) * 4;
        let mut covered_until = 0;
        for span in intersections.as_chunks::<2>().0 {
            let left = ((span[0] - 0.5).ceil() as i32).clamp(0, width);
            let right = ((span[1] - 0.5).ceil() as i32).clamp(0, width);
            if left > covered_until {
                outside.push(row + covered_until as usize * 4..row + left as usize * 4);
            }
            covered_until = right;
        }
        if covered_until < width {
            outside.push(row + covered_until as usize * 4..row + width as usize * 4);
        }
    }
    outside
}

/// Screen device context and a top-down DIB section that frames are copied into.
struct Grabber {
    screen: HDC,
    mem: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    area: RECT,
    length: usize,
    outside: Vec<Range<usize>>,
    cursor: Option<Cursor>,
}

impl Grabber {
    unsafe fn new(area: RECT, points: &[POINT], cursor: bool) -> Option<Grabber> {
        let (width, height) = (area.right - area.left, area.bottom - area.top);
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(mem, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        let old = if bitmap.is_null() || mem.is_null() {
            null_mut()
        } else {
            SelectObject(mem, bitmap)
        };
        let grabber = Grabber {
            screen,
            mem,
            bitmap,
            old,
            bits: bits.cast(),
            area,
            length: width as usize * height as usize * 4,
            outside: outside_runs(width, height, points),
            cursor: cursor.then_some(Cursor {
                icon: null_mut(),
                hotspot: POINT { x: 0, y: 0 },
            }),
        };
        // Dropping releases whatever was created.
        (!screen.is_null() && !mem.is_null() && !bitmap.is_null() && !bits.is_null())
            .then_some(grabber)
    }

    /// Copies the area (with the cursor) and returns its BGRX pixels.
    unsafe fn grab(&mut self) -> &[u8] {
        let (width, height) = (
            self.area.right - self.area.left,
            self.area.bottom - self.area.top,
        );
        BitBlt(
            self.mem,
            0,
            0,
            width,
            height,
            self.screen,
            self.area.left,
            self.area.top,
            SRCCOPY | CAPTUREBLT,
        );
        if let Some(cursor) = &mut self.cursor {
            cursor.draw(self.mem, &self.area);
        }
        GdiFlush();
        let pixels = std::slice::from_raw_parts_mut(self.bits, self.length);
        for run in &self.outside {
            pixels[run.clone()].fill(0);
        }
        pixels
    }
}

impl Drop for Grabber {
    fn drop(&mut self) {
        // SAFETY: each handle is released at most once and only when it was created.
        unsafe {
            if !self.mem.is_null() {
                if !self.old.is_null() {
                    SelectObject(self.mem, self.old);
                }
                DeleteDC(self.mem);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.screen.is_null() {
                ReleaseDC(null_mut(), self.screen);
            }
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

unsafe fn record(
    area: RECT,
    path: &Path,
    stop: &AtomicBool,
    clock: &OnceLock<Instant>,
    points: &[POINT],
    options: Options,
) -> Result<Frame, String> {
    let (width, height) = (area.right - area.left, area.bottom - area.top);
    let mut grabber = Grabber::new(area, points, options.cursor)
        .ok_or(tr("Could not prepare the screen capture."))?;
    let writer = open(path, width, height, options.fps).map_err(|e| {
        format!(
            "{}: {}",
            tr("Could not start the video encoder"),
            e.message()
        )
    })?;
    // Capture only after opening the encoder so the first image is current.
    let mut first = grabber.grab().to_vec();
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
    let frame = Duration::from_nanos(1_000_000_000 / options.fps as u64);
    let mut next = begin;
    #[cfg(test)]
    let (mut captured_frames, mut capture_time, mut encode_time) =
        (1, Duration::ZERO, Duration::ZERO);
    while failure.is_none() && !stop.load(Ordering::Acquire) {
        #[cfg(test)]
        let capture_begin = Instant::now();
        let pixels = grabber.grab();
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
mod tests {
    use super::*;

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
                Options { fps, cursor: true },
            )
            .unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            while recording.started.get().is_none()
                && !recording.failed()
                && Instant::now() < deadline
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
            while recording.started.get().is_none()
                && !recording.failed()
                && Instant::now() < deadline
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
            assert!(samples <= maximum,
                "Encoder warm-up added repeated frames: {samples} frames for {elapsed:?} of recording");
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
            let first_delta =
                u32::from_be_bytes(bytes[timing + 16..timing + 20].try_into().unwrap());
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
}
