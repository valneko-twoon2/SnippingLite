use std::time::{Duration, Instant};

use windows_sys::Win32::{
    Foundation::{BOOL, HWND, LPARAM},
    UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GetClassNameW, GetWindowThreadProcessId,
        InternalGetWindowText, IsWindow, IsWindowVisible, PostMessageW, WM_CLOSE, WM_COMMAND,
    },
};

/// ダイアログが出て閉じるまでを待つ上限。遅い CI でも足りる長さにする。
const DIALOG_LIMIT: Duration = Duration::from_secs(60);
/// 答えを送り直す間隔。開き終える前に送ったメッセージは捨てられる（遅い CI で起きる）ので、
/// ダイアログが閉じるまで送り直す。
const RETRY: Duration = Duration::from_millis(500);
/// 閉じないダイアログの様子を知らせる間隔。
const PROGRESS: Duration = Duration::from_secs(10);

/// このプロセスの、見えている `class`（"#32770" はダイアログ、"#32768" はメニュー）の窓。なければ 0。
/// ほかのプロセス（同時に動くほかのテストや、利用者のアプリ）のダイアログやメニューは数えない。
fn own_visible_window(class: &'static str) -> usize {
    struct Search {
        class: &'static str,
        found: usize,
    }
    unsafe extern "system" fn find(hwnd: HWND, search: LPARAM) -> BOOL {
        let search = &mut *(search as *mut Search);
        if IsWindowVisible(hwnd) != 0 && is_own(hwnd) && class_of(hwnd) == search.class {
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
/// 答えたダイアログが閉じた後にほかのダイアログ（確認やエラー）が残っていれば、それにも同じように
/// 答える。上限を過ぎたら、このプロセスの窓を知らせてダイアログを閉じ、テストを止めずに失敗させる。
pub fn answer_dialog(command: usize) -> std::thread::JoinHandle<bool> {
    let test = std::thread::current().name().unwrap_or("").to_owned();
    // 止まったテストの名前は、CI の画面には出ない（行が終わるまで出ない）ので、ここで知らせる。
    eprintln!("DIALOG waiting for {command} in {test:?}");
    std::thread::spawn(move || {
        let start = Instant::now();
        let (mut answered, mut reported) = (0, start);
        loop {
            let found = own_visible_window("#32770");
            if found == 0 && answered != 0 {
                return true;
            }
            if start.elapsed() > DIALOG_LIMIT {
                eprintln!(
                    "DIALOG for {command} gave up after {} s: {}",
                    DIALOG_LIMIT.as_secs(),
                    own_windows()
                );
                capture_desktop(&test);
                close_own_dialogs();
                return false;
            }
            if found == 0 {
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            if found != answered {
                eprintln!(
                    "DIALOG {found:#x} for {command}: {}",
                    describe(found as HWND)
                );
                answered = found;
            } else if reported.elapsed() > PROGRESS {
                eprintln!(
                    "DIALOG {found:#x} is still open after {} s: {}",
                    start.elapsed().as_secs(),
                    describe(found as HWND)
                );
                reported = Instant::now();
            }
            std::thread::sleep(RETRY);
            // SAFETY: posting a message to a window handle is always sound.
            unsafe {
                if IsWindow(found as HWND) != 0 {
                    // Closing a dialog cancels it, and is the way out of a message box.
                    if command == 2 {
                        PostMessageW(found as HWND, WM_CLOSE, 0, 0);
                    } else {
                        PostMessageW(found as HWND, WM_COMMAND, command, 0);
                    }
                }
            }
        }
    })
}

/// 窓がこのプロセスのものか。
fn is_own(hwnd: HWND) -> bool {
    let mut process = 0;
    // SAFETY: a plain query; an invalid handle only leaves `process` at 0.
    unsafe { GetWindowThreadProcessId(hwnd, &mut process) };
    process == std::process::id()
}

fn class_of(hwnd: HWND) -> String {
    let mut class = [0u16; 64];
    // SAFETY: the buffer outlives the call and its length is passed.
    let length = unsafe { GetClassNameW(hwnd, class.as_mut_ptr(), 64) }.clamp(0, 64) as usize;
    String::from_utf16_lossy(&class[..length])
}

/// 窓の題名。`GetWindowTextW` はこのプロセスの窓に `WM_GETTEXT` を送って答えを待つので、
/// メッセージを処理していないスレッドの窓で止まる。メッセージを送らないほうを使う。
fn text_of(hwnd: HWND) -> String {
    let mut text = [0u16; 256];
    // SAFETY: the buffer outlives the call and its length is passed.
    let length =
        unsafe { InternalGetWindowText(hwnd, text.as_mut_ptr(), 256) }.clamp(0, 256) as usize;
    String::from_utf16_lossy(&text[..length])
}

/// ダイアログの題名と、文字のある子の窓（ボタン、ファイル名の欄、メッセージの文など）。
fn describe(dialog: HWND) -> String {
    unsafe extern "system" fn each(hwnd: HWND, list: LPARAM) -> BOOL {
        let list = &mut *(list as *mut Vec<String>);
        let text = text_of(hwnd);
        if !text.trim().is_empty() && list.len() < 40 {
            list.push(format!("{} {text:?}", class_of(hwnd)));
        }
        1
    }
    let mut children = Vec::new();
    // SAFETY: `children` outlives the synchronous enumeration.
    unsafe {
        EnumChildWindows(
            dialog,
            Some(each),
            &mut children as *mut Vec<String> as LPARAM,
        )
    };
    format!("{:?} [{}]", text_of(dialog), children.join(", "))
}

/// このプロセスの見えている最上位の窓（クラスと題名。ダイアログは子の窓の文字も）。
fn own_windows() -> String {
    unsafe extern "system" fn each(hwnd: HWND, list: LPARAM) -> BOOL {
        let list = &mut *(list as *mut Vec<String>);
        if IsWindowVisible(hwnd) != 0 && is_own(hwnd) {
            let class = class_of(hwnd);
            list.push(if class == "#32770" {
                format!("{hwnd:?} #32770 {}", describe(hwnd))
            } else {
                format!("{hwnd:?} {class} {:?}", text_of(hwnd))
            });
        }
        1
    }
    let mut list = Vec::new();
    // SAFETY: `list` outlives the synchronous enumeration.
    unsafe { EnumWindows(Some(each), &mut list as *mut Vec<String> as LPARAM) };
    list.join("; ")
}

/// このプロセスの見えているダイアログを閉じる。開いたままでは、呼び出し側が戻らず、テストが止まり続ける。
fn close_own_dialogs() {
    unsafe extern "system" fn each(hwnd: HWND, _: LPARAM) -> BOOL {
        if IsWindowVisible(hwnd) != 0 && is_own(hwnd) && class_of(hwnd) == "#32770" {
            PostMessageW(hwnd, WM_CLOSE, 0, 0);
        }
        1
    }
    // SAFETY: the callback only queries and posts to the enumerated windows.
    unsafe { EnumWindows(Some(each), 0) };
}

/// GitHub Actions では、ダイアログを閉じる前のデスクトップを ci-logs/dialog-<テスト名>.png に残す
/// （失敗したジョブの成果物に入る）。ランナーの画面は見えず、ダイアログの中の文は窓の文字から
/// 読めないことがあるので、画像で確かめる。
fn capture_desktop(test: &str) {
    if std::env::var_os("GITHUB_ACTIONS").is_none() {
        return;
    }
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts/ci-screenshot.ps1");
    let name = format!("dialog-{}", test.rsplit("::").next().unwrap_or("test"));
    let status = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(&script)
        .args(["-Name", &name])
        .status();
    eprintln!("DIALOG screenshot {name}: {status:?}");
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
            // 標準のメニューか、範囲のドロップダウンの窓。
            if own_visible_window("#32768") + own_visible_window("SLAreaMenu") == 0 {
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
