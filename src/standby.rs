//! 設定と履歴の窓を、閉じても隠して待たせ、次に開くときは表示し直すだけにする。
//!
//! 窓のプロセスを起こすと、OpenGL の初期化（多くはグラフィックスのドライバーの読み込み）に 200 ms
//! ほどかかり、ボタンを押してから窓が出るまで待たされる。× で閉じたときはプロセスを終えずに窓を
//! 隠し、トレイが次に開くとき（`launch::launch_window`）は隠れた窓を表示するだけにする。
//!
//! 隠れたままのプロセスは、次のときに終える。
//! - 10 分たった: 使わない窓のメモリ（約 70 MB）を持ち続けない。
//! - トレイの窓がなくなった: アプリの終了後に、見えないプロセスだけが残らない。
//!
//! トレイが撮影のあいだ隠した窓（`screenshot::begin_capture`）は、この仕組みで隠したものでは
//! ないので数えない。

use crate::{launch::searched_process, ui, MAIN};
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{IsWindow, IsWindowVisible, ShowWindow, SW_HIDE},
};

/// 隠れたプロセスを終えるまでの時間。
const IDLE: Duration = Duration::from_secs(10 * 60);
/// 隠れたプロセスが、窓とトレイを確かめる間隔。
const CHECK: Duration = Duration::from_secs(1);

/// この仕組みで窓を隠している間だけ true。
static HIDDEN: AtomicBool = AtomicBool::new(false);
/// 見張りのスレッドを起こしたら true。
static WATCHING: AtomicBool = AtomicBool::new(false);

/// 1 つの窓の待機の状態。窓の `ui` で毎フレーム使う。
#[derive(Default)]
pub(crate) struct Standby {
    hidden: bool,
}

impl Standby {
    /// 隠していた窓が表示し直されたら true を返す。呼び出し側は、新しく開いたときと同じ
    /// 中身に戻す（設定を読み直す、履歴を一覧に戻すなど）。
    pub(crate) fn shown_again(&mut self, hwnd: HWND) -> bool {
        // SAFETY: plain window query; a stale handle only reads as not visible.
        if self.hidden && !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) } != 0 {
            self.hidden = false;
            HIDDEN.store(false, Ordering::SeqCst);
            return true;
        }
        false
    }

    /// × で閉じられたら、その閉じる要求を入力から取り除いて、窓を隠す。トレイが動いていない
    /// ときは、次に開き直す相手がいないので、そのまま閉じる。`App::raw_input_hook` から呼ぶ。
    /// eframe は閉じる要求を、`ui` を呼ばない回でも扱い、そこでは `CancelClose` で取り消せない
    /// ので、eframe が要求を読む前の入力から取り除く。
    pub(crate) fn hide_on_close(&mut self, raw_input: &mut egui::RawInput, hwnd: HWND) {
        if !raw_input.viewport().close_requested() || hwnd.is_null() || !tray_running() {
            return;
        }
        for info in raw_input.viewports.values_mut() {
            info.events
                .retain(|event| *event != egui::ViewportEvent::Close);
        }
        // SAFETY: the window belongs to this process.
        unsafe { ShowWindow(hwnd, SW_HIDE) };
        self.hidden = true;
        HIDDEN.store(true, Ordering::SeqCst);
        if !WATCHING.swap(true, Ordering::SeqCst) {
            let window = hwnd as isize;
            std::thread::spawn(move || watch(window as HWND));
        }
    }
}

/// トレイの窓があれば true。
fn tray_running() -> bool {
    // SAFETY: plain window queries.
    !unsafe { ui::find_class_window(MAIN, searched_process()) }.is_null()
}

/// 隠れている間、10 分たつか、トレイがなくなったら、プロセスを終える。表示し直されたら数え直す。
fn watch(hwnd: HWND) {
    let mut since: Option<Instant> = None;
    loop {
        std::thread::sleep(CHECK);
        // SAFETY: plain window queries; the handle belongs to this process.
        let (alive, visible) = unsafe { (IsWindow(hwnd) != 0, IsWindowVisible(hwnd) != 0) };
        if !alive {
            return;
        }
        if visible || !HIDDEN.load(Ordering::SeqCst) {
            since = None;
            continue;
        }
        let hidden_for = since.get_or_insert_with(Instant::now).elapsed();
        if hidden_for >= IDLE || !tray_running() {
            std::process::exit(0);
        }
    }
}
