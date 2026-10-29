//! Helpers shared by the unit tests.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

static ENVIRONMENT: Mutex<()> = Mutex::new(());
/// Windows are looked up by caption, so the tests that create captioned windows take turns.
pub static CAPTIONS: Mutex<()> = Mutex::new(());
/// The theme choice is a process-wide setting.
pub static THEME: Mutex<()> = Mutex::new(());

/// Points `APPDATA`, `LOCALAPPDATA` and `USERPROFILE` at a fresh folder for the length of a test.
/// The environment is shared by every test thread, so one test at a time holds it.
pub struct Sandbox {
    root: PathBuf,
    saved: Vec<(&'static str, Option<OsString>)>,
    _guard: MutexGuard<'static, ()>,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        let guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "snipping-lite-sandbox-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let mut sandbox = Sandbox {
            root,
            saved: Vec::new(),
            _guard: guard,
        };
        let root = sandbox.root.clone();
        sandbox.set("APPDATA", root.join("roaming"));
        sandbox.set("LOCALAPPDATA", root.join("local"));
        sandbox.set("USERPROFILE", root.join("profile"));
        sandbox
    }

    /// Sets another environment variable until the sandbox is dropped.
    pub fn set(&mut self, name: &'static str, value: impl AsRef<std::ffi::OsStr>) {
        self.saved.push((name, std::env::var_os(name)));
        std::env::set_var(name, value);
    }

    pub fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        for (name, value) in self.saved.iter().rev() {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
/// Presses a button of the next dialog this process shows, from another thread:
/// 1 accepts it, 2 cancels or closes it (message boxes only close this way). Returns whether a dialog was found.
pub fn answer_dialog(command: usize) -> std::thread::JoinHandle<bool> {
    use windows_sys::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
            WM_CLOSE, WM_COMMAND,
        },
    };
    unsafe extern "system" fn find(hwnd: HWND, found: LPARAM) -> BOOL {
        let mut class = [0u16; 16];
        let length = GetClassNameW(hwnd, class.as_mut_ptr(), 16) as usize;
        let mut process = 0;
        GetWindowThreadProcessId(hwnd, &mut process);
        if IsWindowVisible(hwnd) != 0
            && process == std::process::id()
            && String::from_utf16_lossy(&class[..length]) == "#32770"
        {
            *(found as *mut usize) = hwnd as usize;
            return 0;
        }
        1
    }
    std::thread::spawn(move || {
        for _ in 0..300 {
            let mut found = 0usize;
            // SAFETY: `found` outlives the synchronous enumeration.
            unsafe { EnumWindows(Some(find), &mut found as *mut usize as LPARAM) };
            if found != 0 {
                eprintln!("DIALOG {found:#x} for {command}");
                // Give the dialog a moment to finish opening.
                std::thread::sleep(std::time::Duration::from_millis(300));
                // SAFETY: posting a message to a window handle is always sound.
                unsafe {
                    // Closing a dialog cancels it, and is the way out of a message box.
                    if command == 2 {
                        PostMessageW(found as HWND, WM_CLOSE, 0, 0);
                    } else {
                        PostMessageW(found as HWND, WM_COMMAND, command, 0);
                    }
                }
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    })
}

/// Types a file name into the next Save As dialog this process shows and accepts it.
pub fn answer_save_as(path: std::path::PathBuf) -> std::thread::JoinHandle<bool> {
    use windows_sys::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumChildWindows, EnumWindows, GetClassNameW, GetWindowThreadProcessId,
            IsWindowVisible, PostMessageW, SendMessageW, WM_COMMAND, WM_SETTEXT,
        },
    };
    fn class_of(hwnd: HWND) -> String {
        let mut class = [0u16; 32];
        // SAFETY: the buffer and its length describe `class`.
        let length = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), 32) } as usize;
        String::from_utf16_lossy(&class[..length])
    }
    unsafe extern "system" fn dialog(hwnd: HWND, found: LPARAM) -> BOOL {
        let mut process = 0;
        GetWindowThreadProcessId(hwnd, &mut process);
        if IsWindowVisible(hwnd) != 0 && process == std::process::id() && class_of(hwnd) == "#32770"
        {
            *(found as *mut usize) = hwnd as usize;
            return 0;
        }
        1
    }
    unsafe extern "system" fn edit(hwnd: HWND, found: LPARAM) -> BOOL {
        if class_of(hwnd) == "Edit" {
            *(found as *mut usize) = hwnd as usize;
            return 0;
        }
        1
    }
    std::thread::spawn(move || {
        for _ in 0..300 {
            let mut found = 0usize;
            // SAFETY: `found` outlives the synchronous enumeration.
            unsafe { EnumWindows(Some(dialog), &mut found as *mut usize as LPARAM) };
            if found != 0 {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let mut field = 0usize;
                let text: Vec<u16> = path
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                // SAFETY: the buffers outlive the calls, which complete before returning.
                unsafe {
                    EnumChildWindows(
                        found as HWND,
                        Some(edit),
                        &mut field as *mut usize as LPARAM,
                    );
                    if field != 0 {
                        SendMessageW(field as HWND, WM_SETTEXT, 0, text.as_ptr() as LPARAM);
                    }
                    PostMessageW(found as HWND, WM_COMMAND, 1, 0);
                }
                return field != 0;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    })
}
/// A hidden window standing in for the settings or history window process, so that opening
/// one finds it instead of starting another copy of the program.
pub fn stub_window(identity: &str) -> windows_sys::Win32::Foundation::HWND {
    use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, SetPropW, WS_POPUP};
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
    // SAFETY: a private window of the system STATIC class; the property is a plain flag.
    unsafe {
        let hwnd = CreateWindowExW(
            0,
            wide("STATIC").as_ptr(),
            wide(crate::SETTINGS_TITLE).as_ptr(),
            WS_POPUP,
            0,
            0,
            10,
            10,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null(),
        );
        assert!(!hwnd.is_null());
        SetPropW(hwnd, wide(identity).as_ptr(), 1usize as _);
        hwnd
    }
}

/// Chooses an entry of the next menu this thread shows: `downs` presses Down before Enter;
/// `usize::MAX` leaves the menu with Escape instead. Returns whether a menu appeared.
pub fn answer_menu(downs: usize) -> std::thread::JoinHandle<bool> {
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_RETURN},
        WindowsAndMessaging::{FindWindowW, PostThreadMessageW, WM_KEYDOWN},
    };
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    // SAFETY: plain query of the calling thread.
    let target = unsafe { GetCurrentThreadId() };
    std::thread::spawn(move || {
        let class: Vec<u16> = "#32768".encode_utf16().chain(Some(0)).collect();
        for _ in 0..300 {
            // SAFETY: the class name is nul-terminated.
            if unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) }.is_null() {
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
            let keys: Vec<u16> = if downs == usize::MAX {
                vec![VK_ESCAPE]
            } else {
                std::iter::repeat_n(VK_DOWN, downs + 1)
                    .chain([VK_RETURN])
                    .collect()
            };
            for key in keys {
                // SAFETY: posting a key to a thread's queue is always sound.
                unsafe { PostThreadMessageW(target, WM_KEYDOWN, key as usize, 0) };
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            return true;
        }
        false
    })
}
/// The image on the clipboard as (width, height, top-down BGRA pixels), if there is one.
pub fn clipboard_image() -> Option<(i32, i32, Vec<u8>)> {
    use windows_sys::Win32::{
        Graphics::Gdi::BITMAPV5HEADER,
        System::{
            DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
            Memory::{GlobalLock, GlobalUnlock},
        },
    };
    // SAFETY: the clipboard is opened and closed here, and the data is copied out while locked.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let global = GetClipboardData(17);
        let image = (!global.is_null()).then(|| {
            let data = GlobalLock(global).cast::<u8>();
            let header = std::ptr::read_unaligned(data.cast::<BITMAPV5HEADER>());
            let size = header.bV5SizeImage as usize;
            let bottom_up =
                std::slice::from_raw_parts(data.add(header.bV5Size as usize), size).to_vec();
            GlobalUnlock(global);
            let row = header.bV5Width as usize * 4;
            let pixels: Vec<u8> = bottom_up
                .chunks_exact(row)
                .rev()
                .flatten()
                .copied()
                .collect();
            (header.bV5Width, header.bV5Height, pixels)
        });
        CloseClipboard();
        image
    }
}

/// Holds the clipboard open for `duration` from another thread, so that copying fails.
pub fn hold_clipboard(duration: std::time::Duration) -> std::thread::JoinHandle<()> {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, OpenClipboard};
    let (ready, wait) = std::sync::mpsc::channel();
    let holder = std::thread::spawn(move || {
        // SAFETY: the clipboard is opened and closed on this thread.
        unsafe {
            let opened = OpenClipboard(std::ptr::null_mut()) != 0;
            ready.send(opened).unwrap();
            std::thread::sleep(duration);
            if opened {
                CloseClipboard();
            }
        }
    });
    assert!(wait.recv().unwrap(), "the clipboard could not be held");
    holder
}
static SHELL_OPENED: Mutex<Vec<(String, Option<String>)>> = Mutex::new(Vec::new());

/// Notes what the program asked the shell to open; nothing is opened while testing.
pub fn shell_opened(target: &[u16], parameters: Option<&[u16]>) {
    let text = |wide: &[u16]| String::from_utf16_lossy(&wide[..wide.len().saturating_sub(1)]);
    SHELL_OPENED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((text(target), parameters.map(text)));
}

/// What was asked of the shell since the last call, oldest first.
pub fn take_shell_opened() -> Vec<(String, Option<String>)> {
    std::mem::take(&mut *SHELL_OPENED.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The Unicode text on the clipboard, if there is any.
pub fn clipboard_text() -> Option<String> {
    use windows_sys::Win32::System::{
        DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
        Memory::{GlobalLock, GlobalUnlock},
    };
    // SAFETY: the clipboard is opened and closed here, and the text is copied out while locked.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let global = GetClipboardData(13);
        let text = (!global.is_null()).then(|| {
            let data = GlobalLock(global).cast::<u16>();
            let mut length = 0;
            while *data.add(length) != 0 {
                length += 1;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(data, length));
            GlobalUnlock(global);
            text
        });
        CloseClipboard();
        text
    }
}
/// An egui screen driven without a window: frames are run by hand, text is located
/// from the painted shapes and the pointer and keyboard are simulated.
pub mod screen {
    use eframe::egui::{
        self, epaint::Shape, Event, Key, Modifiers, PointerButton, Pos2, Rect, Vec2,
    };

    pub struct Screen {
        pub ctx: egui::Context,
        size: Vec2,
        time: f64,
        events: Vec<Event>,
        shapes: Vec<egui::epaint::ClippedShape>,
    }

    pub type Show<'a> = &'a mut dyn FnMut(&mut egui::Ui);

    impl Screen {
        /// A tall screen shows a whole scrolling page without scrolling.
        pub fn new(width: f32, height: f32) -> Screen {
            let ctx = egui::Context::default();
            assert!(
                crate::settings_ui::install_fonts(&ctx),
                "a system font is needed"
            );
            crate::settings_ui::install_visuals(&ctx, false);
            Screen {
                ctx,
                size: Vec2::new(width, height),
                time: 0.0,
                events: Vec::new(),
                shapes: Vec::new(),
            }
        }

        pub fn frame(&mut self, show: Show) {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                ..Default::default()
            };
            input.events = std::mem::take(&mut self.events);
            self.time += 0.1;
            let mut output = self.ctx.run_ui(input, |ui| show(ui));
            // Nothing uploads the textures, so there is nothing to apply.
            output.textures_delta.clear();
            self.shapes = output.shapes;
        }

        /// Runs frames until the layout and animations settle.
        pub fn settle(&mut self, show: Show) {
            for _ in 0..4 {
                self.frame(show);
            }
        }

        /// Every piece of text painted in the last frame, with its rectangle.
        pub fn texts(&self) -> Vec<(String, Rect)> {
            fn collect(shape: &Shape, out: &mut Vec<(String, Rect)>) {
                match shape {
                    Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                    Shape::Text(text) => out.push((
                        text.galley.text().to_owned(),
                        text.galley.rect.translate(text.pos.to_vec2()),
                    )),
                    _ => {}
                }
            }
            let mut out = Vec::new();
            for clipped in &self.shapes {
                collect(&clipped.shape, &mut out);
            }
            out
        }

        /// The rectangles of the pictures painted in the last frame (not text or shapes).
        pub fn images(&self) -> Vec<Rect> {
            fn collect(shape: &Shape, out: &mut Vec<Rect>) {
                match shape {
                    Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                    Shape::Mesh(mesh) => out.push(mesh.calc_bounds()),
                    _ => {}
                }
            }
            let mut out = Vec::new();
            for clipped in &self.shapes {
                collect(&clipped.shape, &mut out);
            }
            out
        }

        /// The centers of the filled circles of this radius painted in the last frame.
        pub fn circles(&self, radius: f32) -> Vec<Pos2> {
            fn collect(shape: &Shape, radius: f32, out: &mut Vec<Pos2>) {
                match shape {
                    Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, radius, out)),
                    Shape::Circle(circle)
                        if circle.radius == radius && circle.fill != egui::Color32::TRANSPARENT =>
                    {
                        out.push(circle.center)
                    }
                    _ => {}
                }
            }
            let mut out = Vec::new();
            for clipped in &self.shapes {
                collect(&clipped.shape, radius, &mut out);
            }
            out
        }
        pub fn find(&self, text: &str) -> Option<Rect> {
            self.texts()
                .into_iter()
                .find(|(t, _)| t == text)
                .map(|(_, r)| r)
        }

        pub fn has(&self, text: &str) -> bool {
            self.find(text).is_some()
        }

        pub fn click(&mut self, at: Pos2, show: Show) {
            self.click_with(at, PointerButton::Primary, show);
        }

        pub fn click_with(&mut self, at: Pos2, button: PointerButton, show: Show) {
            self.events.push(Event::PointerMoved(at));
            self.frame(show);
            self.events.push(Event::PointerButton {
                pos: at,
                button,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
            self.frame(show);
            self.events.push(Event::PointerButton {
                pos: at,
                button,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
            self.frame(show);
            self.settle(show);
        }

        pub fn click_text(&mut self, text: &str, show: Show) {
            let at = self
                .find(text)
                .unwrap_or_else(|| panic!("no text {text:?} on screen: {:?}", self.texts()))
                .center();
            self.click(at, show);
        }

        pub fn drag(&mut self, from: Pos2, to: Pos2, show: Show) {
            self.events.push(Event::PointerMoved(from));
            self.frame(show);
            self.events.push(Event::PointerButton {
                pos: from,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            });
            self.frame(show);
            for step in 1..=4 {
                let at = from + (to - from) * (step as f32 / 4.0);
                self.events.push(Event::PointerMoved(at));
                self.frame(show);
            }
            self.events.push(Event::PointerButton {
                pos: to,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::NONE,
            });
            self.frame(show);
            self.settle(show);
        }

        pub fn hover(&mut self, at: Pos2, show: Show) {
            self.events.push(Event::PointerMoved(at));
            self.frame(show);
            self.settle(show);
        }

        pub fn key(&mut self, key: Key, modifiers: Modifiers, show: Show) {
            for pressed in [true, false] {
                self.events.push(Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed,
                    repeat: false,
                    modifiers,
                });
                self.frame(show);
            }
            self.settle(show);
        }

        pub fn type_text(&mut self, text: &str, show: Show) {
            self.events.push(Event::Text(text.to_owned()));
            self.frame(show);
            self.settle(show);
        }
    }
}

/// The tray process running inside the test: its state, window classes and windows are the
/// real ones, and the test plays the part of the message loop and of the mouse.
pub mod desk {
    use super::{Sandbox, CAPTIONS, THEME};
    use std::{
        sync::MutexGuard,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        UI::{
            HiDpi::{
                SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            },
            WindowsAndMessaging::*,
        },
    };

    pub struct Desk {
        pub sandbox: Sandbox,
        state: Box<crate::State>,
        pub main: HWND,
        dpi: DPI_AWARENESS_CONTEXT,
        _guards: [MutexGuard<'static, ()>; 2],
    }

    impl Desk {
        /// Starts the tray process with the settings saved in a fresh sandbox.
        pub fn start() -> Desk {
            Desk::start_with(|_| {})
        }

        /// Like `start`, after `prepare` changed the saved settings.
        pub fn start_with(prepare: impl FnOnce(&mut crate::config::Settings)) -> Desk {
            // The same order everywhere: theme, captions, then the environment.
            let theme = THEME.lock().unwrap_or_else(|e| e.into_inner());
            let captions = CAPTIONS.lock().unwrap_or_else(|e| e.into_inner());
            let sandbox = Sandbox::new();
            let mut settings = crate::config::parse_settings("");
            // Files go into the sandbox, never into the real Pictures and Videos folders.
            let folder = |name: &str| sandbox.path().join(name).to_string_lossy().into_owned();
            settings.screenshot_folder = folder("shots");
            settings.video_folder = folder("videos");
            prepare(&mut settings);
            crate::config::save_settings(&settings).unwrap();
            // SAFETY: the state and the windows live and die on this thread, one test at a time.
            unsafe {
                let dpi = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                crate::ui::set_theme_choice(settings.theme);
                let mut state = crate::initial_state(&settings);
                crate::STATE = &mut *state;
                crate::register_classes();
                let main = crate::create_main_window();
                assert!(!main.is_null(), "the tray window");
                assert!(crate::create_windows(), "the overlay and the toolbar");
                Desk {
                    sandbox,
                    state,
                    main,
                    dpi,
                    _guards: [theme, captions],
                }
            }
        }

        /// Handles the messages that are waiting, including timers, for this long.
        pub fn pump(&self, duration: Duration) {
            self.pump_until(duration, || false);
        }

        /// Handles messages until `done` holds; false when the time ran out first.
        pub fn pump_until(&self, timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
            let end = Instant::now() + timeout;
            loop {
                // SAFETY: plain message loop calls on this thread's queue.
                unsafe {
                    let mut message: MSG = std::mem::zeroed();
                    while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                if done() {
                    return true;
                }
                if Instant::now() >= end {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }

        pub fn send(&self, hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
            // SAFETY: the window belongs to this thread.
            unsafe { SendMessageW(hwnd, message, wp, lp) }
        }

        /// A mouse message at client coordinates.
        pub fn mouse(&self, hwnd: HWND, message: u32, x: i32, y: i32) -> LRESULT {
            self.send(
                hwnd,
                message,
                0,
                ((y as u16 as isize) << 16) | (x as u16 as isize),
            )
        }

        pub fn click(&self, hwnd: HWND, x: i32, y: i32) {
            self.mouse(hwnd, WM_MOUSEMOVE, x, y);
            self.mouse(hwnd, WM_LBUTTONDOWN, x, y);
            self.mouse(hwnd, WM_LBUTTONUP, x, y);
        }

        /// Presses the left button at `from`, moves through `via` and releases at `to`.
        pub fn drag(&self, hwnd: HWND, from: (i32, i32), via: &[(i32, i32)], to: (i32, i32)) {
            self.mouse(hwnd, WM_MOUSEMOVE, from.0, from.1);
            self.mouse(hwnd, WM_LBUTTONDOWN, from.0, from.1);
            for &(x, y) in via {
                self.mouse(hwnd, WM_MOUSEMOVE, x, y);
            }
            self.mouse(hwnd, WM_MOUSEMOVE, to.0, to.1);
            self.mouse(hwnd, WM_LBUTTONUP, to.0, to.1);
        }

        pub fn key(&self, hwnd: HWND, key: u16) {
            self.send(hwnd, WM_KEYDOWN, key as WPARAM, 0);
        }
    }

    impl Drop for Desk {
        fn drop(&mut self) {
            // SAFETY: the windows were created by this desk on this thread.
            unsafe {
                if let Some(recording) = crate::state().recording.take() {
                    let _ = recording.stop();
                }
                crate::cancel();
                crate::cancel_countdown();
                crate::toast::close();
                for window in [
                    std::mem::replace(&mut crate::state().record_window, std::ptr::null_mut()),
                    std::mem::replace(&mut crate::state().record_frame, std::ptr::null_mut()),
                    crate::state().toolbar,
                    crate::state().overlay,
                    self.main,
                ] {
                    if !window.is_null() {
                        DestroyWindow(window);
                    }
                }
                // Messages for the destroyed windows, such as the quit request, are not wanted.
                let mut message: MSG = std::mem::zeroed();
                while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {}
                crate::STATE = std::ptr::null_mut();
                if !self.dpi.is_null() {
                    SetThreadDpiAwarenessContext(self.dpi);
                }
            }
            let _ = &self.state;
        }
    }
}
