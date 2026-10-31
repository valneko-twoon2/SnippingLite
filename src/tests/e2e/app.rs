use super::*;

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

/// The app under test with its own settings, history and output folders.
pub struct App {
    pub child: Child,
    pub root: PathBuf,
}

impl App {
    pub fn start(name: &str, settings: &str) -> App {
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

    pub fn command(root: &Path) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_snippinglite"));
        command
            .env("APPDATA", root.join("roaming"))
            .env("LOCALAPPDATA", root.join("local"));
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
        Process(
            App::command(&self.root)
                .args(["--history", "--latest"])
                .spawn()
                .expect("Start the history window"),
        )
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
        let _ = self.child.kill();
        let _ = self.child.wait();
        settle(300);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A helper process that is closed when the test ends, even when it fails.
pub struct Process(pub Child);

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
