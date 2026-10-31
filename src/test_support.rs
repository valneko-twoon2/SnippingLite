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
/// このプロセスの、見えている `class`（"#32770" はダイアログ、"#32768" はメニュー）の窓。なければ 0。
/// ほかのプロセス（同時に動くほかのテストや、利用者のアプリ）のダイアログやメニューは数えない。
fn own_visible_window(class: &'static str) -> usize {
    use windows_sys::Win32::{
        Foundation::{BOOL, HWND, LPARAM},
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible,
        },
    };
    struct Search {
        class: &'static str,
        found: usize,
    }
    unsafe extern "system" fn find(hwnd: HWND, search: LPARAM) -> BOOL {
        let search = &mut *(search as *mut Search);
        let mut class = [0u16; 16];
        let length = GetClassNameW(hwnd, class.as_mut_ptr(), 16).clamp(0, 16) as usize;
        let mut process = 0;
        GetWindowThreadProcessId(hwnd, &mut process);
        if IsWindowVisible(hwnd) != 0
            && process == std::process::id()
            && String::from_utf16_lossy(&class[..length]) == search.class
        {
            search.found = hwnd as usize;
            return 0;
        }
        1
    }
    let mut search = Search { class, found: 0 };
    // SAFETY: `search` outlives the synchronous enumeration.
    unsafe { EnumWindows(Some(find), &mut search as *mut Search as LPARAM) };
    search.found
}

/// Presses a button of the next dialog this process shows, from another thread:
/// 1 accepts it, 2 cancels or closes it (message boxes only close this way). Returns whether a dialog was found.
pub fn answer_dialog(command: usize) -> std::thread::JoinHandle<bool> {
    use windows_sys::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{PostMessageW, WM_CLOSE, WM_COMMAND},
    };
    std::thread::spawn(move || {
        for _ in 0..300 {
            let found = own_visible_window("#32770");
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
        WindowsAndMessaging::{PostThreadMessageW, WM_KEYDOWN},
    };
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    // SAFETY: plain query of the calling thread.
    let target = unsafe { GetCurrentThreadId() };
    std::thread::spawn(move || {
        for _ in 0..300 {
            if own_visible_window("#32768") == 0 {
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
/// クリップボードに置かれたファイル（CF_HDROP）の一覧。
pub fn clipboard_files() -> Vec<PathBuf> {
    use windows_sys::Win32::{
        System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard},
        UI::Shell::{DragQueryFileW, HDROP},
    };
    // SAFETY: the clipboard is opened and closed here, and the names are copied out meanwhile.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return Vec::new();
        }
        let drop = GetClipboardData(15) as HDROP;
        let mut files = Vec::new();
        if !drop.is_null() {
            for index in 0..DragQueryFileW(drop, u32::MAX, std::ptr::null_mut(), 0) {
                let mut name = [0u16; 1024];
                let length = DragQueryFileW(drop, index, name.as_mut_ptr(), name.len() as u32);
                files.push(PathBuf::from(String::from_utf16_lossy(
                    &name[..length as usize],
                )));
            }
        }
        CloseClipboard();
        files
    }
}
/// An egui screen driven without a window: frames are run by hand, text is located
/// from the painted shapes and the pointer and keyboard are simulated.
pub mod screen;

/// The tray process running inside the test: its state, window classes and windows are the
/// real ones, and the test plays the part of the message loop and of the mouse.
pub mod desk;

/// ランダムテストのためのシード固定の乱数と、ケースを回して失敗を再現できるようにする仕組み。
pub mod rng;

/// クラッシュ時の整合性のテスト: 子プロセスを書き込みの段階ごとに止めて殺す仕組み。
pub mod crash;

/// テストのバイナリーを子プロセスとして起動し、子のテスト関数を 1 つ走らせる仕組み。
pub mod child;

/// ほかのプロセスで動く Snipping Lite の窓の代わり（子プロセスが作る）。
pub mod others;

/// 壊れた画像のファジングのための PNG の部品（組み立て、分解、参照の読み取り、変異）。
pub mod png;

/// 点 `p` から折れ線 `points` までの距離（f64）。点のない線は `None`。線分ごとに、端点までの
/// 距離と、垂線の足が線分の上にあれば外積から求めた垂線の長さの、小さいほうを取る。線の描画と
/// 消しゴムの判定を、別の方法で確かめるためのもの。
pub fn distance_to_line(points: &[(f32, f32)], (x, y): (f32, f32)) -> Option<f64> {
    let p = (x as f64, y as f64);
    let to = |q: (f64, f64)| ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
    let segment = |a: (f32, f32), b: (f32, f32)| {
        let (a, b) = ((a.0 as f64, a.1 as f64), (b.0 as f64, b.1 as f64));
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = (dx * dx + dy * dy).sqrt();
        let ends = to(a).min(to(b));
        let past_a = (p.0 - a.0) * dx + (p.1 - a.1) * dy > 0.0;
        let before_b = (b.0 - p.0) * dx + (b.1 - p.1) * dy > 0.0;
        if length > 0.0 && past_a && before_b {
            ends.min((dx * (p.1 - a.1) - dy * (p.0 - a.0)).abs() / length)
        } else {
            ends
        }
    };
    match points {
        [] => None,
        [only] => Some(segment(*only, *only)),
        _ => points
            .windows(2)
            .map(|pair| segment(pair[0], pair[1]))
            .reduce(f64::min),
    }
}

/// The GDI and USER objects this process owns, to see whether a flow gives them all back.
pub fn gui_objects() -> (u32, u32) {
    use windows_sys::Win32::Foundation::HANDLE;
    #[link(name = "user32")]
    extern "system" {
        fn GetGuiResources(process: HANDLE, flags: u32) -> u32;
    }
    // -1 is the pseudo handle of the current process; flag 0 counts GDI and 1 USER objects.
    let process = -1isize as HANDLE;
    // SAFETY: a plain query about this process.
    unsafe { (GetGuiResources(process, 0), GetGuiResources(process, 1)) }
}
