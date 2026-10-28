//! Screen recording to MP4: GDI frames go through the Media Foundation sink
//! writer, which encodes H.264 (hardware when available). Only video is recorded.

use std::{
    path::{Path, PathBuf},
    ptr::null_mut,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
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
    Foundation::RECT,
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
}

/// A recording in progress on its own thread.
pub struct Recording {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Result<Frame, String>>,
    pub started: Instant,
    pub path: PathBuf,
}

/// `%USERPROFILE%\Videos\Screen Recordings`, created when missing.
pub fn video_folder() -> std::io::Result<PathBuf> {
    let folder = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("USERPROFILE is unavailable"))?
        .join("Videos")
        .join("Screen Recordings");
    std::fs::create_dir_all(&folder)?;
    Ok(folder)
}

/// Starts recording `area` (screen coordinates); the size is rounded down to
/// even numbers as H.264 requires.
pub fn start(area: RECT, path: PathBuf) -> Result<Recording, String> {
    let width = (area.right - area.left) & !1;
    let height = (area.bottom - area.top) & !1;
    if width < 16 || height < 16 {
        return Err("The selected area is too small to record.".into());
    }
    let area = RECT {
        right: area.left + width,
        bottom: area.top + height,
        ..area
    };
    let stop = Arc::new(AtomicBool::new(false));
    let (ready, started) = mpsc::channel();
    let thread = {
        let (stop, path) = (stop.clone(), path.clone());
        std::thread::spawn(move || run(area, &path, &stop, &ready))
    };
    match started.recv() {
        Ok(Ok(())) => Ok(Recording {
            stop,
            thread,
            started: Instant::now(),
            path,
        }),
        Ok(Err(error)) => {
            let _ = thread.join();
            let _ = std::fs::remove_file(&path);
            Err(error)
        }
        Err(_) => Err("The recorder stopped unexpectedly.".into()),
    }
}

impl Recording {
    /// Ends the recording, finishes the file and returns its first frame.
    pub fn stop(self) -> Result<(PathBuf, Frame), String> {
        self.stop.store(true, Ordering::Release);
        let frame = self
            .thread
            .join()
            .map_err(|_| "The recorder crashed.".to_string())??;
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

fn run(
    area: RECT,
    path: &Path,
    stop: &AtomicBool,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<Frame, String> {
    let (width, height) = (area.right - area.left, area.bottom - area.top);
    // SAFETY: COM and Media Foundation are initialized and shut down on this thread only.
    unsafe {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let started = MFStartup(MF_VERSION, MFSTARTUP_FULL).is_ok();
        let result = record(area, width, height, path, stop, ready);
        if started {
            let _ = MFShutdown();
        }
        if com {
            CoUninitialize();
        }
        if let Err(error) = &result {
            let _ = ready.send(Err(error.clone()));
        }
        result
    }
}

unsafe fn record(
    area: RECT,
    width: i32,
    height: i32,
    path: &Path,
    stop: &AtomicBool,
    ready: &mpsc::Sender<Result<(), String>>,
) -> Result<Frame, String> {
    let writer = open(path, width, height)
        .map_err(|e| format!("Could not start the video encoder: {}", e.message()))?;
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
    if screen.is_null() || mem.is_null() || bitmap.is_null() || bits.is_null() {
        for (handle, is_dc) in [(mem, true), (bitmap, false)] {
            if !handle.is_null() {
                if is_dc {
                    DeleteDC(handle);
                } else {
                    DeleteObject(handle);
                }
            }
        }
        if !screen.is_null() {
            ReleaseDC(null_mut(), screen);
        }
        return Err("Could not prepare the screen capture.".into());
    }
    let old = SelectObject(mem, bitmap);
    let _ = ready.send(Ok(()));
    let length = width as usize * height as usize * 4;
    let frame = std::time::Duration::from_nanos(1_000_000_000 / FPS as u64);
    let begin = Instant::now();
    let mut next = begin;
    let mut first = None;
    let mut failure = None;
    while !stop.load(Ordering::Acquire) {
        BitBlt(
            mem,
            0,
            0,
            width,
            height,
            screen,
            area.left,
            area.top,
            SRCCOPY | CAPTUREBLT,
        );
        draw_cursor(mem, &area);
        GdiFlush();
        let pixels = std::slice::from_raw_parts(bits.cast::<u8>(), length);
        if first.is_none() {
            let mut copy = pixels.to_vec();
            copy.as_chunks_mut::<4>()
                .0
                .iter_mut()
                .for_each(|p| p[3] = 255);
            first = Some(copy);
        }
        let time = begin.elapsed().as_nanos() as i64 / 100;
        if let Err(error) = write(&writer, pixels, time, TICKS / FPS as i64) {
            failure = Some(format!(
                "Could not write a video frame: {}",
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
    SelectObject(mem, old);
    DeleteObject(bitmap);
    DeleteDC(mem);
    ReleaseDC(null_mut(), screen);
    let finalized = writer.sink.Finalize();
    if let Some(error) = failure {
        return Err(error);
    }
    finalized.map_err(|e| format!("Could not finish the video file: {}", e.message()))?;
    first
        .map(|pixels| Frame {
            width,
            height,
            pixels,
        })
        .ok_or_else(|| "No frame was recorded.".to_string())
}
