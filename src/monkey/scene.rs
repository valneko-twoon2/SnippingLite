//! モンキーテストの舞台: ウィンドウ選択の的になる 2 つのウィンドウ、設定と履歴のウィンドウの
//! 代わり、ダイアログとメニューの見張り、無作為な設定、キーとメニューの入力。

use super::*;
use crate::config::{AppTheme, Settings};
use crate::test_support::{rng::Rng, stub_window};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

extern "system" {
    fn GetCurrentThreadId() -> u32;
}

/// 開いたままのメニューを、見張りが Escape で閉じるまでの時間。テストが選ぶキーは先に積んで
/// あるので、これだけ開いているのは、メニューがキーを読まなかったということ。
const MENU_LIMIT: Duration = Duration::from_secs(3);

/// 舞台の一式。ケースごとに作り、ケースの終わりに片付ける。
pub(super) struct Scene {
    pub targets: Targets,
    pub settings: HWND,
    pub history: HWND,
    pub sentry: Sentry,
    /// メニューの中のループから、本物の機器の入力を除くフック。
    menu_filter: HHOOK,
}

/// メニューの中のループが読むメッセージのうち、本物のマウスとキーボードの入力と、本物の時計で
/// 来るこのアプリのタイマーを捨てる（`drive::pump` と同じ）。トレイのメニューは本物のマウスの
/// 位置に開くので、マウスが動くと、テストが積んだキーより先にその下の項目が選ばれてしまう。
/// テストが積んだキーは窓のないメッセージなので通る。
unsafe extern "system" fn filter_menu_input(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    const MSGF_MENU: i32 = 2;
    if code == MSGF_MENU && lp != 0 {
        let message = &*(lp as *const MSG);
        if drive::device_input(message) || drive::real_timer(message) {
            return 1;
        }
    }
    CallNextHookEx(null_mut(), code, wp, lp)
}

impl Scene {
    pub fn new() -> Scene {
        // SAFETY: a hook for this thread only, removed when the scene is dropped.
        let menu_filter = unsafe {
            SetWindowsHookExW(
                WH_MSGFILTER,
                Some(filter_menu_input),
                null_mut(),
                GetCurrentThreadId(),
            )
        };
        assert!(!menu_filter.is_null(), "the menu input filter");
        Scene {
            targets: Targets::new(),
            // 設定と履歴のウィンドウを開く操作は、別のプロセスを起動せずにこれを前に出す。
            settings: stub_window(crate::SETTINGS_WINDOW),
            history: stub_window(crate::HISTORY_WINDOW),
            sentry: Sentry::start(),
            menu_filter,
        }
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        // SAFETY: the stand-ins and the hook were made on this thread by `Scene::new`.
        unsafe {
            DestroyWindow(self.settings);
            DestroyWindow(self.history);
            UnhookWindowsHookEx(self.menu_filter);
        }
    }
}

/// ウィンドウ選択の的: 色を塗るだけの、常に手前に出るポップアップ。後に作った `[1]` が上にあり、
/// 重なった所では上の方が選ばれる。プライマリモニターの左上寄りに置く（ツールバーは上端、
/// 通知は右下に出るので重ならない）。
pub(super) struct Targets {
    pub windows: [HWND; 2],
    /// 画面の座標。
    pub rects: [RECT; 2],
    class: Vec<u16>,
}

unsafe extern "system" fn paint_target(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg != WM_PAINT {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let dc = BeginPaint(hwnd, &mut ps);
    let brush = CreateSolidBrush(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as u32);
    if !brush.is_null() {
        FillRect(dc, &ps.rcPaint, brush);
        DeleteObject(brush);
    }
    EndPaint(hwnd, &ps);
    0
}

impl Targets {
    fn new() -> Targets {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let class = w(&format!(
            "SnippingLiteMonkeyTarget{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let rects = [
            RECT {
                left: 120,
                top: 140,
                right: 640,
                bottom: 480,
            },
            RECT {
                left: 380,
                top: 300,
                right: 680,
                bottom: 500,
            },
        ];
        // SAFETY: a private class and its windows, made and painted on this thread.
        unsafe {
            let spec = WNDCLASSW {
                lpfnWndProc: Some(paint_target),
                hInstance: GetModuleHandleW(null()),
                lpszClassName: class.as_ptr(),
                ..std::mem::zeroed()
            };
            assert_ne!(RegisterClassW(&spec), 0, "the target class");
            let mut windows = [null_mut(); 2];
            for (index, (window, r)) in windows.iter_mut().zip(&rects).enumerate() {
                *window = CreateWindowExW(
                    // 撮る窓に WS_EX_TOOLWINDOW や WS_EX_NOACTIVATE は付けない。窓の撮影が拒む。
                    WS_EX_TOPMOST,
                    class.as_ptr(),
                    w("Snipping Lite monkey target").as_ptr(),
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
                assert!(!window.is_null(), "target {index}");
                let color = [0x0040a020u32, 0x002060d0][index];
                SetWindowLongPtrW(*window, GWLP_USERDATA, color as isize);
                ShowWindow(*window, SW_SHOWNOACTIVATE);
                UpdateWindow(*window);
            }
            DwmFlush();
            Targets {
                windows,
                rects,
                class,
            }
        }
    }

    /// `hwnd` が的なら、ウィンドウを選んだ切り取りの画像の大きさ。最大化していない窓には DWM が
    /// 半透明の 1 px の縁を描くので、四辺から 1 px ずつ除いた大きさになる（`Target::bounds`）。
    pub fn size_of(&self, hwnd: HWND) -> Option<(i32, i32)> {
        let at = self.windows.iter().position(|&window| window == hwnd)?;
        let r = self.rects[at];
        Some((r.right - r.left - 2, r.bottom - r.top - 2))
    }
}

impl Drop for Targets {
    fn drop(&mut self) {
        // SAFETY: the windows and the class were made by `Targets::new` on this thread.
        unsafe {
            for window in self.windows {
                DestroyWindow(window);
            }
            UnregisterClassW(self.class.as_ptr(), GetModuleHandleW(null()));
        }
    }
}

/// このプロセスのダイアログとメニューを見張るスレッド。メッセージボックス（どれも失敗を知らせる
/// もの）は文を記録して閉じ、開いたままのメニューは Escape で閉じて記録する。テストは手ごとに
/// 記録を読み、何かあれば失敗にする。見張りがないと、ダイアログがテストを止めてしまう。
pub(super) struct Sentry {
    stop: Arc<AtomicBool>,
    reports: Arc<Mutex<Vec<String>>>,
    thread: Option<JoinHandle<()>>,
}

/// EnumWindows に渡す、見つけたもの。
struct Found {
    process: u32,
    dialogs: Vec<usize>,
    menu: bool,
}

unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
    let found = &mut *(data as *mut Found);
    let mut process = 0;
    GetWindowThreadProcessId(hwnd, &mut process);
    if process != found.process || IsWindowVisible(hwnd) == 0 {
        return 1;
    }
    let mut class = [0u16; 16];
    let length = GetClassNameW(hwnd, class.as_mut_ptr(), 16).clamp(0, 16) as usize;
    match String::from_utf16_lossy(&class[..length]).as_str() {
        "#32770" => found.dialogs.push(hwnd as usize),
        "#32768" => found.menu = true,
        _ => {}
    }
    1
}

/// ダイアログの文。メッセージボックスの文は ID 0xFFFF の子にある。
unsafe fn dialog_text(dialog: HWND) -> String {
    let mut text = [0u16; 512];
    let mut length = 0usize;
    let child = GetDlgItem(dialog, 0xFFFF);
    if !child.is_null() {
        SendMessageTimeoutW(
            child,
            WM_GETTEXT,
            text.len(),
            text.as_mut_ptr() as LPARAM,
            SMTO_ABORTIFHUNG,
            500,
            &mut length,
        );
    }
    String::from_utf16_lossy(&text[..length.min(text.len())])
}

impl Sentry {
    fn start() -> Sentry {
        let stop = Arc::new(AtomicBool::new(false));
        let reports = Arc::new(Mutex::new(Vec::new()));
        // SAFETY: a plain query of the calling thread.
        let target = unsafe { GetCurrentThreadId() };
        let thread = {
            let (stop, reports) = (stop.clone(), reports.clone());
            std::thread::spawn(move || {
                let mut handled: Vec<usize> = Vec::new();
                let mut menu_since: Option<Instant> = None;
                while !stop.load(Ordering::Relaxed) {
                    let mut found = Found {
                        process: std::process::id(),
                        dialogs: Vec::new(),
                        menu: false,
                    };
                    // SAFETY: `found` outlives the synchronous enumeration; the dialog is closed
                    // by posting, and its text is read with a timeout.
                    unsafe {
                        EnumWindows(Some(collect), &mut found as *mut Found as LPARAM);
                        for dialog in found.dialogs {
                            if handled.contains(&dialog) {
                                continue;
                            }
                            handled.push(dialog);
                            let text = dialog_text(dialog as HWND);
                            if let Ok(mut list) = reports.lock() {
                                list.push(format!("a message box said {text:?}"));
                            }
                            PostMessageW(dialog as HWND, WM_CLOSE, 0, 0);
                        }
                        if !found.menu {
                            menu_since = None;
                        } else if menu_since.get_or_insert_with(Instant::now).elapsed() > MENU_LIMIT
                        {
                            menu_since = None;
                            PostThreadMessageW(target, WM_KEYDOWN, VK_ESCAPE as usize, 0);
                            if let Ok(mut list) = reports.lock() {
                                list.push(
                                    "a menu stayed open; the chosen keys were not read".into(),
                                );
                            }
                        }
                    }
                    std::thread::sleep(Duration::from_millis(15));
                }
            })
        };
        Sentry {
            stop,
            reports,
            thread: Some(thread),
        }
    }

    /// 前に読んでから記録したこと。
    pub fn take(&self) -> Vec<String> {
        self.reports
            .lock()
            .map(|mut list| std::mem::take(&mut *list))
            .unwrap_or_default()
    }
}

impl Drop for Sentry {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// 設定ウィンドウが書くような、無作為な設定。届く操作のうち、テストの外に出るものだけは固定する:
/// 名前を付けて保存のダイアログは開かず（ask）、録画はカウントダウンを経て（countdown）、音は録らない
/// （mic、system_audio。録音はマイクと出力の機器しだいで、別のテストが確かめる）。
/// 保存先はサンドボックスのまま変えない。外した設定（遅延、フレームレート）も書き、使われない
/// ことを確かめる。
pub(super) fn randomize(rng: &mut Rng, settings: &mut Settings) {
    settings.copy = rng.chance(0.7);
    // 保存がオンで履歴がオフのときだけ、通知のクリックが保存したファイルを開く。
    settings.save = rng.chance(0.6);
    settings.ask = false;
    settings.delay = *rng.pick(&config::DELAYS);
    settings.history = rng.chance(0.5);
    settings.history_limit = *rng.pick(&config::HISTORY_LIMITS);
    settings.countdown = true;
    settings.skip_title = rng.chance(0.5);
    settings.record_cursor = rng.chance(0.5);
    settings.record_fps = *rng.pick(&config::FRAME_RATES);
    settings.freeze = rng.chance(0.5);
    settings.notify = rng.chance(0.8);
    settings.keep_toolbar = rng.chance(0.5);
    settings.outline = rng.chance(0.3);
    settings.outline_color = rng.below(annotate::COLORS.len());
    settings.outline_width = rng.below(annotate::OUTLINE_WIDTHS.len());
    settings.mic = false;
    settings.system_audio = false;
    settings.app_audio = rng.chance(0.5);
    settings.copy_video = rng.chance(0.5);
    settings.save_video = rng.chance(0.5);
    settings.theme = *rng.pick(&AppTheme::ALL);
}

/// `control` のときは Ctrl を押したままにして `send` を呼ぶ。オーバーレイが読む GetKeyState は
/// このスレッドのキーの状態なので、本物のキーボードには触れない。
pub(super) fn with_control<T>(control: bool, send: impl FnOnce() -> T) -> T {
    let mut keys = [0u8; 256];
    // SAFETY: both calls read or write the 256 key states of this thread.
    unsafe {
        GetKeyboardState(keys.as_mut_ptr());
        let saved = keys;
        keys[VK_CONTROL as usize] = if control { 0x80 } else { 0 };
        SetKeyboardState(keys.as_ptr());
        let result = send();
        SetKeyboardState(saved.as_ptr());
        result
    }
}

/// 次に開くメニューで押すキーを、このスレッドのキューに先に積む。メニューは開いたあとの
/// ループでキューから順に読む。`Some(n)` は上から n 番目（0 から）を選び、`None` は Escape で閉じる。
pub(super) fn queue_menu(choice: Option<usize>) {
    let keys: Vec<u16> = match choice {
        None => vec![VK_ESCAPE],
        Some(downs) => std::iter::repeat_n(VK_DOWN, downs + 1)
            .chain([VK_RETURN])
            .collect(),
    };
    // SAFETY: posting to this thread's own queue is always sound.
    unsafe {
        let target = GetCurrentThreadId();
        for key in keys {
            PostThreadMessageW(target, WM_KEYDOWN, key as usize, 0);
        }
    }
}
