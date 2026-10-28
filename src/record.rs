//! Screen recording to MP4: GDI frames go through the Media Foundation sink
//! writer, which encodes H.264 (hardware when available). Only video is recorded.

use crate::i18n::tr;
use std::{
    ops::Range,
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
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
        DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, DI_NORMAL, ICONINFO,
    },
};

const FPS: u32 = 30;
/// Hundreds of nanoseconds per second, the unit of Media Foundation times.
const TICKS: i64 = 10_000_000;

/// The first frame of a finished recording, used as the notification preview.
pub struct Frame {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
    /// Set when the recording ended early but the file up to that point was kept.
    pub warning: Option<String>,
}

/// A recording in progress on its own thread.
pub struct Recording {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Result<Frame, String>>,
    pub started: Instant,
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
pub fn start(area: RECT, path: PathBuf, points: Vec<POINT>) -> Result<Recording, String> {
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
    // The encoder starts on the recorder thread; the clock starts now.
    let started = Instant::now();
    let thread = {
        let (stop, path) = (stop.clone(), path.clone());
        std::thread::spawn(move || run(area, &path, &stop, started, &points))
    };
    Ok(Recording {
        stop,
        thread,
        started,
        path,
    })
}

impl Recording {
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
) -> windows::core::Result<IMFMediaType> {
    // SAFETY: plain Media Foundation calls on objects created here.
    unsafe {
        let media = MFCreateMediaType()?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        media.SetGUID(&MF_MT_SUBTYPE, subtype)?;
        media.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        media.SetUINT64(&MF_MT_FRAME_SIZE, ((width as u64) << 32) | height as u64)?;
        media.SetUINT64(&MF_MT_FRAME_RATE, ((FPS as u64) << 32) | 1)?;
        media.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1 << 32) | 1)?;
        Ok(media)
    }
}

fn open(path: &Path, width: i32, height: i32) -> windows::core::Result<Writer> {
    // SAFETY: plain Media Foundation calls on objects created here.
    unsafe {
        let mut attributes = None;
        MFCreateAttributes(&mut attributes, 1)?;
        let attributes = attributes.ok_or_else(windows::core::Error::empty)?;
        attributes.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
        let url = HSTRING::from(path.to_string_lossy().as_ref());
        let sink = MFCreateSinkWriterFromURL(&url, None, &attributes)?;
        let output = video_type(&MFVideoFormat_H264, width, height)?;
        let bitrate = (width as f64 * height as f64 * FPS as f64 * 0.1).clamp(2e6, 20e6) as u32;
        output.SetUINT32(&MF_MT_AVG_BITRATE, bitrate)?;
        let stream = sink.AddStream(&output)?;
        let input = video_type(&MFVideoFormat_RGB32, width, height)?;
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

/// Draws the mouse cursor, which BitBlt does not include, into the frame.
unsafe fn draw_cursor(dc: HDC, area: &RECT) {
    let mut info: CURSORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
    if GetCursorInfo(&mut info) == 0 || info.flags & CURSOR_SHOWING == 0 {
        return;
    }
    let mut icon: ICONINFO = std::mem::zeroed();
    if GetIconInfo(info.hCursor, &mut icon) == 0 {
        return;
    }
    DrawIconEx(
        dc,
        info.ptScreenPos.x - icon.xHotspot as i32 - area.left,
        info.ptScreenPos.y - icon.yHotspot as i32 - area.top,
        info.hCursor,
        0,
        0,
        0,
        null_mut(),
        DI_NORMAL,
    );
    if !icon.hbmMask.is_null() {
        DeleteObject(icon.hbmMask);
    }
    if !icon.hbmColor.is_null() {
        DeleteObject(icon.hbmColor);
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
}

impl Grabber {
    unsafe fn new(area: RECT, points: &[POINT]) -> Option<Grabber> {
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
        draw_cursor(self.mem, &self.area);
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
    begin: Instant,
    points: &[POINT],
) -> Result<Frame, String> {
    // SAFETY: COM and Media Foundation are initialized and shut down on this thread only.
    unsafe {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let started = MFStartup(MF_VERSION, MFSTARTUP_FULL).is_ok();
        let result = record(area, path, stop, begin, points);
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
    begin: Instant,
    points: &[POINT],
) -> Result<Frame, String> {
    let (width, height) = (area.right - area.left, area.bottom - area.top);
    let mut grabber =
        Grabber::new(area, points).ok_or(tr("Could not prepare the screen capture."))?;
    // The first frame is taken at once; the encoder needs a moment to start.
    let mut first = grabber.grab().to_vec();
    first
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .for_each(|p| p[3] = 255);
    let writer = open(path, width, height).map_err(|e| {
        format!(
            "{}: {}",
            tr("Could not start the video encoder"),
            e.message()
        )
    })?;
    let duration = TICKS / FPS as i64;
    let mut failure = write(&writer, &first, 0, duration)
        .err()
        .map(|e| format!("{}: {}", tr("Could not write a video frame"), e.message()));
    // A failure after the first frame still leaves a playable file to keep.
    let started = failure.is_none();
    let frame = Duration::from_nanos(1_000_000_000 / FPS as u64);
    let mut next = Instant::now();
    while failure.is_none() && !stop.load(Ordering::Acquire) {
        let pixels = grabber.grab();
        let time = begin.elapsed().as_nanos() as i64 / 100;
        if let Err(error) = write(&writer, pixels, time, duration) {
            failure = Some(format!(
                "{}: {}",
                tr("Could not write a video frame"),
                error.message()
            ));
            break;
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
        (Some(error), Ok(())) if started => Some(error),
        (Some(error), _) => return Err(error),
        (None, Err(error)) => return Err(error),
        (None, Ok(())) => None,
    };
    Ok(Frame {
        width,
        height,
        pixels: first,
        warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires an unlocked Windows desktop and Media Foundation encoder"]
    fn records_rectangular_and_freeform_mp4() {
        for freeform in [false, true] {
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
            )
            .expect("Start the recording worker");
            std::thread::sleep(Duration::from_secs(1));
            let (_, frame) = recording.stop().expect("Finalize the MP4 recording");
            let bytes = std::fs::read(&path).expect("Read the generated MP4");
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
