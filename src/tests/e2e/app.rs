use super::*;

/// アプリを起動し直す回数の上限。起動したアプリがショートカットを登録できなかったとき
/// （ほかのテストのプロセスがその瞬間だけ持っていたとき）や、トレイを渡して終わったときに起動し直す。
const START_ATTEMPTS: usize = 3;
/// ショートカットとトレイの窓が空くのを待つ秒数。ほかのテストのプロセスが一時的に持つことがある。
const FREE_SECONDS: u64 = 30;
/// 止めたプロセスが終わるのを待つ秒数。
const EXIT_SECONDS: u64 = 10;

pub fn hotkey_taken() -> bool {
    // SAFETY: a thread hotkey that is released right away.
    unsafe {
        if RegisterHotKey(null_mut(), 0x5e2e, MOD_ALT | MOD_SHIFT | MOD_NOREPEAT, 0x53) == 0 {
            return true;
        }
        UnregisterHotKey(null_mut(), 0x5e2e);
        false
    }
}

/// どこかのプロセスにトレイの窓（`SnippingLiteMain`）があるか。あれば、起動したアプリはそこへ
/// 設定を開く知らせを送って終わる（動いている Snipping Lite に触れることになる）。
fn tray_exists() -> bool {
    let class = wide("SnippingLiteMain");
    // SAFETY: a plain window query; the class name is nul-terminated.
    !unsafe { FindWindowW(class.as_ptr(), null()) }.is_null()
}

/// プロセスを止め、終わるのを上限つきで待つ。
fn stop(child: &mut Child) {
    let _ = child.kill();
    let end = Instant::now() + Duration::from_secs(EXIT_SECONDS);
    while !matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
        if Instant::now() >= end {
            eprintln!("プロセス {} が {EXIT_SECONDS} 秒で終わらない", child.id());
            break;
        }
        settle(50);
    }
    disown(child.id());
}

/// The app under test with its own settings, history and output folders.
pub struct App {
    pub child: Child,
    pub root: PathBuf,
}

impl App {
    pub fn start(name: &str, settings: &str) -> App {
        let root =
            std::env::temp_dir().join(format!("snippinglite-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for folder in ["roaming/Snipping Lite", "local", "temp", "shots", "videos"] {
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
        for attempt in 1..=START_ATTEMPTS {
            let free = wait_for(FREE_SECONDS, || {
                (!hotkey_taken() && !tray_exists()).then_some(())
            });
            if free.is_none() {
                let _ = std::fs::remove_dir_all(&root);
                panic!("Alt+Shift+S is in use or another tray is running: close Snipping Lite before running the end-to-end tests");
            }
            let mut child = App::command(&root).spawn().expect("Start the app");
            adopt(child.id());
            settle(500);
            // 起動したアプリが動き続け、ショートカットを登録したら始める。
            let started = wait_for(15, || match child.try_wait() {
                Ok(None) => hotkey_taken().then_some(true),
                _ => Some(false),
            });
            if started == Some(true) {
                return App { child, root };
            }
            eprintln!(
                "{attempt} 回目に起動したアプリが Alt+Shift+S を登録しなかった（{}）",
                if started.is_some() {
                    "終わった"
                } else {
                    "15 秒待った"
                }
            );
            stop(&mut child);
        }
        let _ = std::fs::remove_dir_all(&root);
        panic!("The app did not register Alt+Shift+S");
    }

    /// テストするアプリのプロセスを、テストの設定・履歴・一時フォルダーで起動する命令。
    /// 一時フォルダーも差し替えるので、保存しない録画（1 日より古いものを消す）や、バージョン情報の
    /// ページが、利用者の本物の一時フォルダーに書かれない。
    pub fn command(root: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_snippinglite"));
        command
            .env("APPDATA", root.join("roaming"))
            .env("LOCALAPPDATA", root.join("local"))
            .env("TEMP", root.join("temp"))
            .env("TMP", root.join("temp"));
        command
    }

    /// テストするアプリのトレイの窓（なければ null）。同じクラスのほかのプロセスの窓（ユニット
    /// テストのトレイや、動いている Snipping Lite）は数えない。
    pub fn tray(&self) -> HWND {
        let class = wide("SnippingLiteMain");
        let mut previous = null_mut();
        // 同じクラスの最上位の窓はわずかなので、上限で打ち切っても取りこぼさない。
        for _ in 0..1000 {
            // SAFETY: plain window queries; the class name is nul-terminated.
            let window = unsafe { FindWindowExW(null_mut(), previous, class.as_ptr(), null()) };
            if window.is_null() {
                return window;
            }
            let mut process = 0;
            // SAFETY: as above.
            unsafe { GetWindowThreadProcessId(window, &mut process) };
            if process == self.child.id() {
                return window;
            }
            previous = window;
        }
        null_mut()
    }

    /// Opens the history window on the newest screenshot.
    pub fn history(&self) -> Process {
        let child = App::command(&self.root)
            .args(["--history", "--latest"])
            .spawn()
            .expect("Start the history window");
        adopt(child.id());
        Process(child)
    }

    pub fn files(&self, folder: &str, extension: &str) -> Vec<PathBuf> {
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
        stop(&mut self.child);
        settle(300);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A helper process that is closed when the test ends, even when it fails.
pub struct Process(pub Child);

impl Drop for Process {
    fn drop(&mut self) {
        stop(&mut self.0);
    }
}
