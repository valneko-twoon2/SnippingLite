use super::*;

/// アプリを起動し直す回数の上限。起動したアプリがショートカットを登録できなかったとき
/// （ほかのテストのプロセスがその瞬間だけ持っていたとき）や、トレイを渡して終わったときに起動し直す。
const START_ATTEMPTS: usize = 3;
/// ショートカットとトレイの窓が空き、ほかのプロセスの設定・履歴・プレイヤーの窓がなくなるのを
/// 待つ秒数。ほかのテストのプロセスが一時的に持つことがある。
const FREE_SECONDS: u64 = 30;
/// 止めたプロセスが終わるのを待つ秒数。
const EXIT_SECONDS: u64 = 10;
/// 設定・履歴・プレイヤーの窓の題名と、それぞれの役目の印（`launch.rs` の `SETTINGS_TITLE` と `WINDOWS`）。
const APP_TITLE: &str = "Snipping Lite";
const APP_WINDOWS: [&str; 3] = [
    "SnippingLite.Settings",
    "SnippingLite.History",
    "SnippingLite.Player",
];
/// 題名が `APP_TITLE` の最上位の窓を数える上限。
const TITLED_LIMIT: usize = 1000;
/// テストが起動するアプリの写しを置く、一時フォルダーの中のフォルダー。
const EXE_FOLDER: &str = "snippinglite-e2e-exe";
/// 写しを書けるようになるのを待つ秒数。写した直後の exe はウイルス対策の検査などで少しの間
/// 書けないことがある。
const COPY_SECONDS: u64 = 10;
/// テストが起動するアプリの写しのパス（`exe`）。
static EXE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// テストが起動するアプリ。ビルドした exe を、どの worktree やビルドから実行しても同じパス
/// （`%TEMP%\snippinglite-e2e-exe\snippinglite.exe`）へ写して、そこから起動する。アプリがトレイに
/// アイコンを足すと、Windows は exe のパスごとに `HKCU\Control Panel\NotifyIconSettings` に項目を
/// 残し、設定の「そのほかのシステム トレイ アイコン」に並べる。ビルドしたパス（worktree ごとの
/// `src/target`）のまま起動すると、E2E を走らせた worktree の数だけ Snipping Lite が増える。
/// 写しはテストの後も残し（次の実行が同じパスを使う）、同じビルドなら写し直さない。
/// ほかのプロセスが写しを動かしていないことを確かめてから（`App::start`）呼ぶ。
fn exe() -> &'static Path {
    EXE.get_or_init(|| {
        let built = Path::new(env!("CARGO_BIN_EXE_snippinglite"));
        let folder = std::env::temp_dir().join(EXE_FOLDER);
        let copy = folder.join("snippinglite.exe");
        // 写しは元の更新時刻を引き継ぐので、大きさと更新時刻が同じなら同じビルド。
        let stamp = |path: &Path| {
            std::fs::metadata(path)
                .ok()
                .map(|m| (m.len(), m.modified().ok()))
        };
        let current = stamp(&copy);
        if current.is_none() || current != stamp(built) {
            std::fs::create_dir_all(&folder).expect("Create the folder for the app");
            let copied = wait_for(COPY_SECONDS, || std::fs::copy(built, &copy).ok());
            assert!(
                copied.is_some(),
                "Copy the app to {} (is a process still running it?)",
                copy.display()
            );
        }
        copy
    })
}

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

/// このテストとテストするアプリのプロセスではない、Snipping Lite の設定・履歴・プレイヤーの窓が
/// あるか（隠れた窓も数える）。テストするアプリはテスト用のビルドではないので、撮影のたびに、
/// すべてのプロセスからこれらの窓を探して隠し、撮影の後に表示に戻す（`screenshot.rs`）。利用者が
/// トレイだけを終了して履歴の窓を開いたままにしていると、その窓が撮影のたびに隠れ、撮影の途中で
/// テストが落ちてアプリを止めると、隠れたまま残る。探し方は `ui::find_app_window` と同じ。
fn app_window_exists() -> bool {
    let title = wide(APP_TITLE);
    let marks: Vec<Vec<u16>> = APP_WINDOWS.iter().map(|mark| wide(mark)).collect();
    let mut previous = null_mut();
    for _ in 0..TITLED_LIMIT {
        // SAFETY: plain window queries; the strings are nul-terminated.
        let window = unsafe { FindWindowExW(null_mut(), previous, null(), title.as_ptr()) };
        if window.is_null() {
            return false;
        }
        // SAFETY: as above.
        let marked = marks
            .iter()
            .any(|mark| !unsafe { GetPropW(window, mark.as_ptr()) }.is_null());
        if marked && !ours(window) {
            return true;
        }
        previous = window;
    }
    // 数えきれないほどあれば、あるものとして扱う（利用者の窓を見落として始めない）。
    true
}

/// プロセスを止め、終わるのを上限つきで待つ。
fn stop(child: &mut Child) {
    let _ = child.kill();
    let end = Instant::now() + Duration::from_secs(EXIT_SECONDS);
    while !matches!(child.try_wait(), Ok(Some(_)) | Err(_)) {
        if Instant::now() >= end {
            report(format_args!(
                "プロセス {} が {EXIT_SECONDS} 秒で終わらない",
                child.id()
            ));
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
                (!hotkey_taken() && !tray_exists() && !app_window_exists()).then_some(())
            });
            if free.is_none() {
                let _ = std::fs::remove_dir_all(&root);
                panic!("Alt+Shift+S is in use or another tray or window of Snipping Lite is open: close Snipping Lite before running the end-to-end tests");
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
            report(format_args!(
                "{attempt} 回目に起動したアプリが Alt+Shift+S を登録しなかった（{}）",
                if started.is_some() {
                    "終わった"
                } else {
                    "15 秒待った"
                }
            ));
            stop(&mut child);
        }
        let _ = std::fs::remove_dir_all(&root);
        panic!("The app did not register Alt+Shift+S");
    }

    /// テストするアプリのプロセスを、テストの設定・履歴・一時フォルダーで起動する命令。
    /// 一時フォルダーも差し替えるので、保存しない録画（1 日より古いものを消す）や、バージョン情報の
    /// ページが、利用者の本物の一時フォルダーに書かれない。exe は決まったパスへの写し（`exe`）。
    pub fn command(root: &Path) -> Command {
        let mut command = Command::new(exe());
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

    /// Opens the history window on the newest screenshot. Its error output goes to a file in the
    /// test folder, which `Process::report` reads when the window does not show up.
    pub fn history(&self) -> Process {
        let errors = std::fs::File::create(self.root.join("history.err")).ok();
        let mut command = App::command(&self.root);
        command.args(["--history", "--latest"]);
        if let Some(errors) = errors {
            command.stderr(errors);
        }
        let child = command.spawn().expect("Start the history window");
        adopt(child.id());
        Process {
            child,
            errors: self.root.join("history.err"),
        }
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
pub struct Process {
    pub child: Child,
    errors: PathBuf,
}

impl Process {
    /// プロセスが生きているか終わったかと、その標準エラー出力。窓が出ないときの原因を失敗に添える。
    pub fn report(&mut self) -> String {
        let state = match self.child.try_wait() {
            Ok(None) => "still running".to_string(),
            Ok(Some(status)) => format!("exited with {status}"),
            Err(error) => format!("unknown ({error})"),
        };
        let errors = std::fs::read_to_string(&self.errors).unwrap_or_default();
        format!("process {state}; stderr: {:?}", errors.trim())
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        stop(&mut self.child);
    }
}
