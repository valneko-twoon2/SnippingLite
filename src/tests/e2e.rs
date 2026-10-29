//! End-to-end tests of the real app.
//!
//! Each test starts the built app with its own settings, history and output
//! folders, covers the primary monitor with a backdrop of known colors, drives
//! the app with synthetic mouse and keyboard input like a user and checks what
//! reaches the clipboard, the disk and the screen. They take over the mouse and
//! keyboard and need an unlocked desktop where nothing else uses Alt+Shift+S,
//! so they are ignored by default. Run them one at a time with
//! `cargo test --manifest-path src/Cargo.toml --config src/.cargo/config.toml --test e2e -- --ignored --test-threads=1`.

use std::{
    path::{Path, PathBuf},
    process::{Child, Command},
    ptr::{null, null_mut},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, MutexGuard, Once,
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::*,
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard},
        LibraryLoader::GetModuleHandleW,
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
    },
    UI::{
        HiDpi::{
            GetDpiForMonitor, SetProcessDpiAwarenessContext,
            DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
        },
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::*,
    },
};

/// The backdrop color as the screen reports it (0xRRGGBB).
const BACKGROUND: u32 = 0x336699;
/// The backdrop under the dimmed snipping overlay, which halves every channel.
const DIMMED: u32 = (BACKGROUND >> 1) & 0x7f7f7f;
const CYAN: u32 = 0x00ffff;
/// The first color of the border setting.
const RED: u32 = 0xe81123;
/// A white panel with text and a cyan square, in pixels from the monitor's top-left corner.
const PANEL: RECT = RECT {
    left: 60,
    top: 200,
    right: 700,
    bottom: 420,
};
const SQUARE: RECT = RECT {
    left: 600,
    top: 260,
    right: 680,
    bottom: 340,
};
const TEXT: &str = "Hello World 123";

/// Toolbar positions in DIPs from its left edge, as in `toolbar.rs`.
const CAMERA: f32 = 26.0;
const VIDEO: f32 = 62.0;
const AREA: f32 = 116.0;
const CLOSE: f32 = 306.0;
/// The space between the capture types and the area button, which moves the toolbar.
const GAP: f32 = 86.0;
/// Recording indicator positions in DIPs, as in `video.rs`.
const TIMER: f32 = 60.0;
const STOP: f32 = 130.0;

/// The tests share the screen, the mouse and the shortcut.
static DESKTOP: Mutex<()> = Mutex::new(());
static PANEL_SHOWN: AtomicBool = AtomicBool::new(false);
static SETUP: Once = Once::new();

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Handles the backdrop's messages for `ms` milliseconds; every wait goes through
/// here so the window keeps painting and answering the app's activation messages.
fn settle(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        // SAFETY: a plain message loop for windows of this thread.
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if Instant::now() >= end {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Polls `check` until it gives a value or the time is up.
fn wait_for<T>(seconds: u64, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(value) = check() {
            return Some(value);
        }
        if Instant::now() >= end {
            return None;
        }
        settle(100);
    }
}

fn near(a: u32, b: u32, tolerance: u32) -> bool {
    (0..3).all(|i| ((a >> (i * 8)) & 0xff).abs_diff((b >> (i * 8)) & 0xff) <= tolerance)
}

fn colorref(rgb: u32) -> u32 {
    ((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff)
}

fn offset(r: RECT, x: i32, y: i32) -> RECT {
    RECT {
        left: r.left + x,
        top: r.top + y,
        right: r.right + x,
        bottom: r.bottom + y,
    }
}

fn center(r: RECT) -> POINT {
    POINT {
        x: (r.left + r.right) / 2,
        y: (r.top + r.bottom) / 2,
    }
}

unsafe fn fill(dc: HDC, r: RECT, rgb: u32) {
    let brush = CreateSolidBrush(colorref(rgb));
    FillRect(dc, &r, brush);
    DeleteObject(brush);
}

unsafe extern "system" fn backdrop_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut client);
            fill(dc, client, BACKGROUND);
            if PANEL_SHOWN.load(Ordering::Relaxed) {
                fill(dc, PANEL, 0xffffff);
                fill(dc, SQUARE, CYAN);
                let font = CreateFontW(
                    -64,
                    0,
                    0,
                    0,
                    FW_NORMAL as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    OUT_DEFAULT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    ANTIALIASED_QUALITY as u32,
                    DEFAULT_PITCH as u32,
                    wide("Segoe UI").as_ptr(),
                );
                let old = SelectObject(dc, font);
                SetBkMode(dc, TRANSPARENT as i32);
                SetTextColor(dc, 0);
                let mut area = RECT {
                    left: PANEL.left + 20,
                    top: PANEL.top,
                    right: SQUARE.left - 10,
                    bottom: PANEL.bottom,
                };
                let mut text: Vec<u16> = TEXT.encode_utf16().collect();
                DrawTextW(
                    dc,
                    text.as_mut_ptr(),
                    text.len() as i32,
                    &mut area,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
                SelectObject(dc, old);
                DeleteObject(font);
            }
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// A window of a known color over the whole primary monitor, so captures have
/// known pixels and a stray click lands on it instead of another app.
struct Backdrop {
    hwnd: HWND,
    monitor: RECT,
    scale: f32,
}

impl Backdrop {
    fn new() -> Backdrop {
        // SAFETY: the window belongs to this thread and is destroyed on drop.
        unsafe {
            let class = wide("SnippingLiteE2EBackdrop");
            SETUP.call_once(|| {
                SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(backdrop_proc),
                    hInstance: GetModuleHandleW(null()),
                    hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                    lpszClassName: class.as_ptr(),
                    ..std::mem::zeroed()
                });
            });
            let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            let mut info: MONITORINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
            GetMonitorInfoW(monitor, &mut info);
            let (mut dpi, mut unused) = (96u32, 96u32);
            GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused);
            let r = info.rcMonitor;
            PANEL_SHOWN.store(false, Ordering::Relaxed);
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!hwnd.is_null(), "Create the backdrop");
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            settle(500);
            Backdrop {
                hwnd,
                monitor: r,
                scale: dpi as f32 / 96.0,
            }
        }
    }

    fn width(&self) -> i32 {
        self.monitor.right - self.monitor.left
    }

    fn height(&self) -> i32 {
        self.monitor.bottom - self.monitor.top
    }

    /// A screen point `(x, y)` pixels from the monitor's top-left corner.
    fn at(&self, x: i32, y: i32) -> POINT {
        POINT {
            x: self.monitor.left + x,
            y: self.monitor.top + y,
        }
    }

    fn rect(&self, r: RECT) -> RECT {
        offset(r, self.monitor.left, self.monitor.top)
    }

    fn show_panel(&self, shown: bool) {
        PANEL_SHOWN.store(shown, Ordering::Relaxed);
        // SAFETY: the window belongs to this thread.
        unsafe {
            InvalidateRect(self.hwnd, null(), 1);
            UpdateWindow(self.hwnd);
        }
        settle(300);
    }

    fn topmost(&self, on: bool) {
        // SAFETY: the window belongs to this thread.
        unsafe {
            SetWindowPos(
                self.hwnd,
                if on { HWND_TOPMOST } else { HWND_NOTOPMOST },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE,
            );
        }
        settle(200);
    }

    /// The toolbar where a newly opened snip places it: the top center of the monitor.
    fn toolbar(&self) -> RECT {
        let width = (332.0 * self.scale).round() as i32;
        let left = (self.monitor.left + self.monitor.right - width) / 2;
        let top = self.monitor.top + (12.0 * self.scale).round() as i32;
        RECT {
            left,
            top,
            right: left + width,
            bottom: top + (48.0 * self.scale).round() as i32,
        }
    }

    /// The recording indicator at the top center of the monitor.
    fn indicator(&self) -> RECT {
        let width = (168.0 * self.scale).round() as i32;
        let left = (self.monitor.left + self.monitor.right - width) / 2;
        let top = self.monitor.top + (12.0 * self.scale).round() as i32;
        RECT {
            left,
            top,
            right: left + width,
            bottom: top + (48.0 * self.scale).round() as i32,
        }
    }

    /// A point `x` DIPs from the left of a bar, in the middle of its height.
    fn button(&self, bar: RECT, x: f32) -> POINT {
        POINT {
            x: bar.left + (x * self.scale).round() as i32,
            y: bar.top + (24.0 * self.scale).round() as i32,
        }
    }

    /// A free part of the backdrop for selections, right of center.
    fn free(&self) -> POINT {
        self.at(self.width() / 2 + 120, self.height() / 2)
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        // SAFETY: the window belongs to this thread.
        unsafe { DestroyWindow(self.hwnd) };
        settle(100);
    }
}

fn hotkey_taken() -> bool {
    // SAFETY: a thread hotkey that is released right away.
    unsafe {
        if RegisterHotKey(null_mut(), 0x5e2e, MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, 0x53) == 0 {
            return true;
        }
        UnregisterHotKey(null_mut(), 0x5e2e);
        false
    }
}

/// The app under test with its own settings, history and output folders.
struct App {
    child: Child,
    root: PathBuf,
}

impl App {
    fn start(name: &str, settings: &str) -> App {
        assert!(
            !hotkey_taken(),
            "Alt+Shift+S is in use: close Snipping Lite before running the end-to-end tests"
        );
        let root =
            std::env::temp_dir().join(format!("snippinglite-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for folder in ["roaming/Snipping Lite", "local", "shots", "videos"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        std::fs::write(
            root.join("roaming/Snipping Lite/settings.ini"),
            format!(
                "{settings}\r\nscreenshot_folder={}\r\nvideo_folder={}\r\n",
                root.join("shots").display(),
                root.join("videos").display()
            ),
        )
        .unwrap();
        let child = App::command(&root).spawn().expect("Start the app");
        let app = App { child, root };
        settle(500);
        assert!(
            wait_for(15, || hotkey_taken().then_some(())).is_some(),
            "The app did not register Alt+Shift+S"
        );
        app
    }

    fn command(root: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_snippinglite"));
        command
            .env("APPDATA", root.join("roaming"))
            .env("LOCALAPPDATA", root.join("local"));
        command
    }

    /// Opens the history window on the newest screenshot.
    fn history(&self) -> Process {
        Process(
            App::command(&self.root)
                .args(["--history", "--latest"])
                .spawn()
                .expect("Start the history window"),
        )
    }

    fn files(&self, folder: &str, extension: &str) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(self.root.join(folder))
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| {
                        p.extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case(extension))
                    })
                    .collect()
            })
            .unwrap_or_default();
        files.sort();
        files
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        settle(300);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A helper process that is closed when the test ends, even when it fails.
struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn send_mouse(flags: MOUSE_EVENT_FLAGS) {
    // SAFETY: one fully initialized input record.
    unsafe {
        let mut input: INPUT = std::mem::zeroed();
        input.r#type = INPUT_MOUSE;
        input.Anonymous.mi.dwFlags = flags;
        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
    }
}

fn move_to(p: POINT) {
    // SAFETY: moving the cursor has no memory effects.
    unsafe { SetCursorPos(p.x, p.y) };
}

fn click(p: POINT) {
    move_to(p);
    settle(150);
    send_mouse(MOUSEEVENTF_LEFTDOWN);
    settle(60);
    send_mouse(MOUSEEVENTF_LEFTUP);
    settle(500);
}

/// Presses the left button at the first point, moves through the others and lets go at the last.
fn drag(path: &[POINT]) {
    move_to(path[0]);
    settle(150);
    send_mouse(MOUSEEVENTF_LEFTDOWN);
    settle(150);
    for pair in path.windows(2) {
        for step in 1..=12 {
            move_to(POINT {
                x: pair[0].x + (pair[1].x - pair[0].x) * step / 12,
                y: pair[0].y + (pair[1].y - pair[0].y) * step / 12,
            });
            settle(15);
        }
    }
    settle(100);
    send_mouse(MOUSEEVENTF_LEFTUP);
    settle(800);
}

fn select(from: POINT, width: i32, height: i32) {
    drag(&[
        from,
        POINT {
            x: from.x + width,
            y: from.y + height,
        },
    ]);
}

/// Presses the keys together and releases them in reverse.
fn keys(keys: &[VIRTUAL_KEY]) {
    let input = |key: VIRTUAL_KEY, up: bool| {
        // SAFETY: a fully initialized keyboard input record.
        unsafe {
            let mut input: INPUT = std::mem::zeroed();
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki.wVk = key;
            input.Anonymous.ki.dwFlags = if up { KEYEVENTF_KEYUP } else { 0 };
            SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
        }
    };
    for &key in keys {
        input(key, false);
        settle(20);
    }
    for &key in keys.iter().rev() {
        input(key, true);
        settle(20);
    }
    settle(250);
}

fn shortcut() {
    keys(&[VK_MENU, VK_SHIFT, 0x53]);
    settle(800);
}

/// The screen pixels of `r` as 0xRRGGBB, top-down.
fn screen(r: RECT) -> Vec<u32> {
    let (width, height) = (r.right - r.left, r.bottom - r.top);
    // SAFETY: the bitmap is sized for the area and all GDI objects are released here.
    unsafe {
        let screen = GetDC(null_mut());
        let dc = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        assert!(
            !bitmap.is_null() && !bits.is_null(),
            "Create a capture bitmap"
        );
        let old = SelectObject(dc, bitmap);
        BitBlt(
            dc,
            0,
            0,
            width,
            height,
            screen,
            r.left,
            r.top,
            SRCCOPY | CAPTUREBLT,
        );
        GdiFlush();
        let pixels = std::slice::from_raw_parts(bits.cast::<u32>(), (width * height) as usize)
            .iter()
            .map(|p| p & 0xffffff)
            .collect();
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(null_mut(), screen);
        pixels
    }
}

/// Whether something other than the plain or dimmed backdrop covers most of `r`.
fn covered(r: RECT) -> bool {
    let inner = RECT {
        left: r.left + 4,
        top: r.top + 4,
        right: r.right - 4,
        bottom: r.bottom - 4,
    };
    let pixels = screen(inner);
    let other = pixels
        .iter()
        .filter(|&&p| !near(p, BACKGROUND, 12) && !near(p, DIMMED, 12))
        .count();
    other * 10 > pixels.len() * 4
}

/// Whether the snipping overlay dims the backdrop around `p`.
fn dimmed(p: POINT) -> bool {
    screen(RECT {
        left: p.x,
        top: p.y,
        right: p.x + 4,
        bottom: p.y + 4,
    })
    .iter()
    .all(|&c| near(c, DIMMED, 12))
}

/// The bounds of every pixel of exactly `rgb` on all monitors.
fn find(rgb: u32) -> Option<RECT> {
    // SAFETY: plain metrics queries.
    let desktop = unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        RECT {
            left,
            top,
            right: left + GetSystemMetrics(SM_CXVIRTUALSCREEN),
            bottom: top + GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    };
    let width = (desktop.right - desktop.left) as usize;
    let mut bounds: Option<RECT> = None;
    for (i, _) in screen(desktop)
        .iter()
        .enumerate()
        .filter(|(_, &p)| p == rgb)
    {
        let (x, y) = (
            desktop.left + (i % width) as i32,
            desktop.top + (i / width) as i32,
        );
        let b = bounds.get_or_insert(RECT {
            left: x,
            top: y,
            right: x + 1,
            bottom: y + 1,
        });
        b.left = b.left.min(x);
        b.top = b.top.min(y);
        b.right = b.right.max(x + 1);
        b.bottom = b.bottom.max(y + 1);
    }
    bounds
}

/// Opens the clipboard; another app may hold it for a moment.
fn open_clipboard() -> bool {
    for _ in 0..40 {
        // SAFETY: opening without an owner window is allowed.
        if unsafe { OpenClipboard(null_mut()) } != 0 {
            return true;
        }
        settle(25);
    }
    false
}

fn clear_clipboard() {
    assert!(open_clipboard(), "Open the clipboard");
    // SAFETY: the clipboard is open.
    unsafe {
        EmptyClipboard();
        CloseClipboard();
    }
}

/// The clipboard image (the app's 32-bit DIB) as top-down BGRA pixels.
fn clipboard_image() -> Option<(i32, i32, Vec<u8>)> {
    if !open_clipboard() {
        return None;
    }
    // SAFETY: the data is only read while locked and within its allocation size.
    unsafe {
        let handle = GetClipboardData(17);
        let mut image = None;
        let data = if handle.is_null() {
            null_mut()
        } else {
            GlobalLock(handle).cast::<u8>()
        };
        if !data.is_null() {
            let header = std::ptr::read_unaligned(data.cast::<BITMAPV5HEADER>());
            let (width, height) = (header.bV5Width, header.bV5Height);
            let row = width.max(0) as usize * 4;
            let length = row * height.unsigned_abs() as usize;
            if header.bV5Size as usize + length <= GlobalSize(handle) {
                let bytes = std::slice::from_raw_parts(data.add(header.bV5Size as usize), length);
                let pixels: Vec<u8> = if height > 0 {
                    bytes.chunks_exact(row).rev().flatten().copied().collect()
                } else {
                    bytes.to_vec()
                };
                image = Some((width, height.abs(), pixels));
            }
            GlobalUnlock(handle);
        }
        CloseClipboard();
        image
    }
}

fn clipboard_text() -> Option<String> {
    if !open_clipboard() {
        return None;
    }
    // SAFETY: the text is read up to its terminator within the allocation while locked.
    unsafe {
        let handle = GetClipboardData(13);
        let mut text = None;
        let data = if handle.is_null() {
            null_mut()
        } else {
            GlobalLock(handle).cast::<u16>()
        };
        if !data.is_null() {
            let units = std::slice::from_raw_parts(data, GlobalSize(handle) / 2);
            let length = units.iter().position(|&u| u == 0).unwrap_or(units.len());
            text = Some(String::from_utf16_lossy(&units[..length]));
            GlobalUnlock(handle);
        }
        CloseClipboard();
        text
    }
}

/// The color and alpha of a pixel of top-down BGRA pixels.
fn pixel(pixels: &[u8], width: i32, x: i32, y: i32) -> (u32, u8) {
    let p = &pixels[(y * width + x) as usize * 4..][..4];
    (
        (u32::from(p[2]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[0]),
        p[3],
    )
}

/// The size in a PNG file's header.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    let data = std::fs::read(path).ok()?;
    if data.get(..8)? != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let number = |at: usize| Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?));
    Some((number(16)?, number(20)?))
}

/// The length of a finished MP4 file from its movie header; `None` while it is still recorded.
fn mp4_seconds(path: &Path) -> Option<f64> {
    let data = std::fs::read(path).ok()?;
    if data.get(4..8)? != b"ftyp" {
        return None;
    }
    let at = data.windows(4).position(|w| w == b"mvhd")? + 4;
    let number = |at: usize, size: usize| {
        data.get(at..at + size)
            .map(|b| b.iter().fold(0u64, |n, &byte| (n << 8) | u64::from(byte)))
    };
    let (scale, duration) = if *data.get(at)? == 1 {
        (number(at + 20, 4)?, number(at + 24, 8)?)
    } else {
        (number(at + 12, 4)?, number(at + 16, 4)?)
    };
    (scale > 0).then(|| duration as f64 / scale as f64)
}

/// Takes the desktop for one test and covers the primary monitor.
fn desktop() -> (MutexGuard<'static, ()>, Backdrop) {
    let guard = DESKTOP.lock().unwrap_or_else(|e| e.into_inner());
    (guard, Backdrop::new())
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn a_selection_reaches_the_clipboard_and_the_screenshot_folder() {
    let (_desktop, backdrop) = desktop();
    for freeze in [false, true] {
        let app = App::start(
            &format!("snip-{freeze}"),
            &format!(
                "copy=1\r\nsave=1\r\nask=0\r\nhistory=0\r\nnotify=0\r\nfreeze={}",
                u8::from(freeze)
            ),
        );
        clear_clipboard();
        shortcut();
        assert!(covered(backdrop.toolbar()), "The toolbar opens");
        assert_eq!(
            dimmed(backdrop.free()),
            freeze,
            "Dimming follows the setting"
        );
        select(backdrop.free(), 240, 160);
        let (width, height, pixels) =
            wait_for(10, clipboard_image).expect("A screenshot on the clipboard");
        assert_eq!((width, height), (240, 160), "freeze {freeze}");
        for y in 0..height {
            for x in 0..width {
                let (color, alpha) = pixel(&pixels, width, x, y);
                assert!(
                    near(color, BACKGROUND, 4) && alpha == 255,
                    "{x},{y} is {color:06x} with alpha {alpha}"
                );
            }
        }
        let png = wait_for(10, || app.files("shots", "png").into_iter().next())
            .expect("A PNG in the screenshot folder");
        assert_eq!(png_size(&png), Some((240, 160)));
        assert!(!covered(backdrop.toolbar()), "The toolbar closes");
        assert!(!dimmed(backdrop.free()), "The overlay closes");
    }
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn the_border_is_drawn_inside_rectangles_and_not_around_freeform_shapes() {
    let (_desktop, backdrop) = desktop();
    let _app = App::start(
        "border",
        "copy=1\r\nsave=0\r\nhistory=0\r\nnotify=0\r\nfreeze=0\r\noutline=1\r\noutline_color=0\r\noutline_width=2",
    );
    clear_clipboard();
    shortcut();
    select(backdrop.free(), 200, 120);
    let (width, height, pixels) = wait_for(10, clipboard_image).expect("A screenshot");
    assert_eq!((width, height), (200, 120));
    // The thick border is 4 pixels wide, inside the edges.
    for y in 0..height {
        for x in 0..width {
            let edge = x < 4 || y < 4 || x >= width - 4 || y >= height - 4;
            let (color, _) = pixel(&pixels, width, x, y);
            let expected = if edge { RED } else { BACKGROUND };
            assert!(near(color, expected, 4), "{x},{y} is {color:06x}");
        }
    }
    // Choose Freeform in the area menu with the keyboard, then draw a triangle.
    clear_clipboard();
    shortcut();
    click(backdrop.button(backdrop.toolbar(), AREA));
    keys(&[VK_DOWN]);
    keys(&[VK_DOWN]);
    keys(&[VK_RETURN]);
    let start = backdrop.free();
    drag(&[
        POINT {
            x: start.x,
            y: start.y + 150,
        },
        POINT {
            x: start.x + 100,
            y: start.y,
        },
        POINT {
            x: start.x + 200,
            y: start.y + 150,
        },
        POINT {
            x: start.x + 2,
            y: start.y + 148,
        },
    ]);
    let (width, height, pixels) = wait_for(10, clipboard_image).expect("A freeform screenshot");
    assert_eq!((width, height), (200, 150));
    assert_eq!(
        pixel(&pixels, width, 0, 0).1,
        0,
        "Outside the shape is transparent"
    );
    assert_eq!(pixel(&pixels, width, width - 1, 0).1, 0);
    let (inside, alpha) = pixel(&pixels, width, 100, 120);
    assert!(near(inside, BACKGROUND, 4) && alpha == 255, "{inside:06x}");
    assert!(
        (0..height).all(|y| (0..width).all(|x| !near(pixel(&pixels, width, x, y).0, RED, 40))),
        "A freeform shape gets no border"
    );
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn a_kept_toolbar_returns_moves_stays_out_of_screenshots_and_closes() {
    let (_desktop, backdrop) = desktop();
    let _app = App::start(
        "keep",
        "copy=1\r\nsave=0\r\nhistory=0\r\nnotify=0\r\nfreeze=1\r\nkeep_toolbar=1",
    );
    let home = backdrop.toolbar();
    let probe = backdrop.free();
    shortcut();
    assert!(covered(home) && dimmed(probe), "A snip opens");
    keys(&[VK_ESCAPE]);
    assert!(covered(home), "Escape keeps the toolbar");
    assert!(!dimmed(probe), "Escape ends the snip");

    // Drag the space between the buttons.
    let moved = offset(home, -300, 250);
    let grip = backdrop.button(home, GAP);
    drag(&[
        grip,
        POINT {
            x: grip.x - 300,
            y: grip.y + 250,
        },
    ]);
    assert!(covered(moved), "The toolbar follows the drag");
    assert!(!covered(home), "The toolbar left its old place");

    // A new snip keeps the moved toolbar, and a selection over its place does not show it.
    clear_clipboard();
    shortcut();
    assert!(covered(moved) && !covered(home) && dimmed(probe));
    let from = POINT {
        x: moved.left - 30,
        y: moved.top - 30,
    };
    let (width, height) = (moved.right - moved.left + 60, moved.bottom - moved.top + 60);
    select(from, width, height);
    let (w, h, pixels) = wait_for(10, clipboard_image).expect("A screenshot");
    assert_eq!((w, h), (width, height));
    let toolbar = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| {
            let color = (u32::from(p[2]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[0]);
            !near(color, BACKGROUND, 4)
        })
        .count();
    assert_eq!(toolbar, 0, "The kept toolbar is not in the screenshot");
    assert!(covered(moved), "The toolbar returns after the snip");
    assert!(!dimmed(probe));

    // Its camera button starts the next snip.
    click(backdrop.button(moved, CAMERA));
    assert!(dimmed(probe), "The camera button starts a snip");
    keys(&[VK_ESCAPE]);
    assert!(!dimmed(probe) && covered(moved));

    // The close button closes it; the next snip opens it at the top center again.
    click(backdrop.button(moved, CLOSE));
    assert!(
        !covered(moved) && !covered(home),
        "The close button closes the toolbar"
    );
    shortcut();
    assert!(
        covered(home) && !covered(moved),
        "A reopened toolbar starts at the top center"
    );
    keys(&[VK_ESCAPE]);
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn a_recording_ends_only_from_the_stop_button() {
    let (_desktop, backdrop) = desktop();
    let app = App::start(
        "record",
        "copy=1\r\nsave=0\r\nhistory=0\r\nnotify=0\r\nfreeze=0\r\ncountdown=0\r\nmic=0\r\nsystem_audio=0\r\nrecord_cursor=0\r\nrecord_fps=30",
    );
    shortcut();
    click(backdrop.button(backdrop.toolbar(), VIDEO));
    select(backdrop.free(), 320, 200);
    settle(1000);
    let indicator = backdrop.indicator();
    click(backdrop.button(indicator, TIMER));
    settle(1500);
    let files = app.files("videos", "mp4");
    assert_eq!(files.len(), 1, "One recording is running");
    assert!(
        mp4_seconds(&files[0]).is_none(),
        "A click on the elapsed time does not end the recording"
    );
    click(backdrop.button(indicator, STOP));
    let seconds =
        wait_for(15, || mp4_seconds(&files[0])).expect("The Stop button finishes the recording");
    assert!(
        seconds >= 2.0,
        "The video covers the whole recording: {seconds} s"
    );
    assert_eq!(app.files("videos", "mp4").len(), 1);
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn recording_areas_below_the_encoder_minimum_are_ignored() {
    let (_desktop, backdrop) = desktop();
    let app = App::start(
        "small",
        "copy=1\r\nsave=0\r\nhistory=0\r\nnotify=0\r\nfreeze=1\r\ncountdown=0\r\nmic=0\r\nsystem_audio=0\r\nrecord_cursor=0\r\nrecord_fps=30",
    );
    shortcut();
    click(backdrop.button(backdrop.toolbar(), VIDEO));
    let probe = POINT {
        x: backdrop.free().x + 200,
        y: backdrop.free().y + 200,
    };
    for size in [20, 33] {
        select(backdrop.free(), size, size);
        assert!(
            app.files("videos", "mp4").is_empty(),
            "{size} px starts no recording"
        );
        assert!(dimmed(probe), "The snip stays open after {size} px");
    }
    select(backdrop.free(), 34, 34);
    settle(1200);
    assert_eq!(app.files("videos", "mp4").len(), 1, "34 px records");
    shortcut();
    let files = app.files("videos", "mp4");
    let seconds = wait_for(15, || mp4_seconds(&files[0])).expect("The shortcut finishes it");
    assert!(seconds > 0.5, "{seconds} s");
}

#[test]
#[ignore = "Takes over the mouse, the keyboard and the screen"]
fn the_history_view_copies_text_and_colors() {
    let (_desktop, backdrop) = desktop();
    let app = App::start(
        "history",
        "copy=1\r\nsave=0\r\nhistory=1\r\nnotify=0\r\nfreeze=0",
    );
    backdrop.show_panel(true);
    clear_clipboard();
    shortcut();
    let panel = backdrop.rect(PANEL);
    select(
        POINT {
            x: panel.left,
            y: panel.top,
        },
        panel.right - panel.left,
        panel.bottom - panel.top,
    );
    let (width, _, pixels) = wait_for(10, clipboard_image).expect("A screenshot of the panel");
    let square = offset(SQUARE, -PANEL.left, -PANEL.top);
    assert_eq!(
        pixel(&pixels, width, square.left + 5, square.top + 5).0,
        CYAN
    );
    wait_for(10, || {
        (!app.files("local/Snipping Lite/history", "png").is_empty()).then_some(())
    })
    .expect("A history entry");

    // Only the history window shows the square from now on.
    backdrop.show_panel(false);
    backdrop.topmost(false);
    click(backdrop.at(backdrop.width() / 2, backdrop.height() / 2 + 200));
    // SAFETY: lets the new process take the foreground from this one.
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    let _history = app.history();
    let shown = wait_for(30, || find(CYAN)).expect("The history window shows the screenshot");
    // Clicking the image without a tool only activates the window.
    click(center(shown));

    // Tab order: Back, Delete, Show in folder, Save as..., Copy, Copy text.
    clear_clipboard();
    for _ in 0..6 {
        keys(&[VK_TAB]);
    }
    keys(&[VK_RETURN]);
    let text = wait_for(30, || clipboard_text().filter(|t| !t.is_empty()))
        .expect("Copy text puts the recognized text on the clipboard");
    assert!(text.contains("World"), "{text:?}");

    // Then the tools: Pen, Highlighter, Crop, Pick color.
    clear_clipboard();
    for _ in 0..4 {
        keys(&[VK_TAB]);
    }
    keys(&[VK_RETURN]);
    click(center(shown));
    let color = wait_for(10, clipboard_text).expect("Pick color copies the color");
    assert_eq!(color, "#00FFFF");
}
