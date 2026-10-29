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
/// 1 accepts it, 2 cancels it. Returns whether a dialog was found.
pub fn answer_dialog(command: usize) -> std::thread::JoinHandle<bool> {
    use windows_sys::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
            WM_COMMAND,
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
                // Give the dialog a moment to finish opening.
                std::thread::sleep(std::time::Duration::from_millis(300));
                // SAFETY: posting a command to a window handle is always sound.
                unsafe { PostMessageW(found as HWND, WM_COMMAND, command, 0) };
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    })
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
