//! 窓を開く処理のテスト。開いていない窓は、もう 1 つのプログラムを、その窓が知るべきこと
//! （新しいスクリーンショットを開くか、ショートカットを登録できたか）を引数に付けて起動する。
//! 開いている窓は前に出すだけで、何も起動しない。
//!
//! テストの中では、試験のプログラム（`current_exe`）が製品の代わりに起動される。試験のプログラムは
//! 知らない引数を受け取ると、最初の 1 つを標準エラーに「Unrecognized option: '<名前>'」と書いて
//! 終わる。起動の間だけ、このプロセスの標準エラーをパイプに向けておくと、子がそれを受け継ぐので、
//! 子が書いたものを読んで、どの引数が渡ったかを確かめられる。

use super::*;
use crate::test_support::{desk::Desk, stub_window};
use std::{
    io::{PipeWriter, Read as _},
    os::windows::io::AsRawHandle as _,
    time::Duration,
};
use windows_sys::Win32::Foundation::{BOOL, HANDLE};

#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(id: u32) -> HANDLE;
    fn SetStdHandle(id: u32, handle: HANDLE) -> BOOL;
}
/// `STD_ERROR_HANDLE`（-12）。
const STD_ERROR: u32 = -12i32 as u32;
/// どの窓にも付いていない印。これを探すと、窓は見つからない。
const NOT_OPEN: &str = "SnippingLite.Test.NotOpen";

/// 生きている間、このプロセスの標準エラーをパイプの書き口に向ける。落とすと元に戻す（途中で
/// テストが失敗しても、後のテストの出力をパイプへ流さない）。
struct Redirected {
    saved: HANDLE,
    _writer: PipeWriter,
}

impl Redirected {
    fn to(writer: PipeWriter) -> Redirected {
        // SAFETY: 書き口はこの値と一緒に生き、落とす前に元の標準エラーへ戻す。
        unsafe {
            let saved = GetStdHandle(STD_ERROR);
            assert_ne!(
                SetStdHandle(STD_ERROR, writer.as_raw_handle() as HANDLE),
                0,
                "the standard error points at the pipe"
            );
            Redirected {
                saved,
                _writer: writer,
            }
        }
    }
}

impl Drop for Redirected {
    fn drop(&mut self) {
        // SAFETY: 起動の前に取っておいた、このプロセスの標準エラーに戻す。
        unsafe { SetStdHandle(STD_ERROR, self.saved) };
    }
}

/// `launch` が起動した子が標準エラーに書いたもの。子が終わるとパイプが閉じるので、そこまで読む。
/// 子を起動しなければ、書き口はこのプロセスにしかないので、すぐ空で終わる。
fn started_with(launch: impl FnOnce()) -> String {
    let (mut reader, writer) = std::io::pipe().expect("a pipe");
    {
        let _redirected = Redirected::to(writer);
        launch();
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = reader.read_to_string(&mut text);
        let _ = sender.send(text);
    });
    receiver
        .recv_timeout(Duration::from_secs(60))
        .expect("the started copy ends")
}

fn shown(window: HWND) -> bool {
    // SAFETY: plain window query.
    unsafe { IsWindowVisible(window) != 0 }
}

#[test]
#[ignore = "Creates a process"]
fn a_window_that_is_not_open_starts_a_copy_that_knows_what_to_show() {
    let mut desk = Desk::start();
    // 子の測定データは、テストのものと混ざらないよう、自分のファイルに書かせる。
    let profile = desk.sandbox.path().join("child-%p.profraw");
    desk.sandbox.set("LLVM_PROFILE_FILE", profile);
    let reported = |text: &str, option: &str| text.contains(&format!("option: '{option}'"));
    // SAFETY: the state belongs to this test.
    unsafe {
        crate::state().hotkey_ok = true;
        // 設定と履歴は、それぞれの引数を最初に付けて起動する。
        let said = started_with(|| launch_settings());
        assert!(reported(&said, "settings"), "{said}");
        let said = started_with(|| launch_history(true));
        assert!(reported(&said, "history"), "{said}");
        // 続く引数を見るため、最初の引数には試験のプログラムが受け付ける `--list` を使う。報告される
        // のは、その次の引数。新しいスクリーンショットを開くよう頼まれたときだけ `--latest` が続く。
        let said = started_with(|| launch_window(NOT_OPEN, "--list", true));
        assert!(reported(&said, "latest"), "{said}");
        // 頼まれなければ、次はショートカットの状態。登録できていれば `--hotkey-ok`。
        let said = started_with(|| launch_window(NOT_OPEN, "--list", false));
        assert!(reported(&said, "hotkey-ok"), "{said}");
        // ほかのアプリが使っていて登録できなければ `--hotkey-busy`。設定の画面は、これを見て
        // ショートカットの衝突を知らせる。
        crate::state().hotkey_ok = false;
        let said = started_with(|| launch_window(NOT_OPEN, "--list", false));
        assert!(reported(&said, "hotkey-busy"), "{said}");
        let said = started_with(|| launch_settings());
        assert!(reported(&said, "settings"), "{said}");
    }
}

#[test]
#[ignore = "Creates the tray window"]
fn an_open_window_comes_forward_and_no_other_copy_starts() {
    let _desk = Desk::start();
    let settings = stub_window(SETTINGS_WINDOW);
    let history = stub_window(HISTORY_WINDOW);
    // SAFETY: the windows belong to this test.
    unsafe {
        ShowWindow(settings, SW_HIDE);
        ShowWindow(history, SW_HIDE);
        let said = started_with(|| launch_settings());
        assert!(shown(settings) && !shown(history));
        assert_eq!(said, "", "nothing was started");
        let said = started_with(|| launch_history(true));
        assert!(shown(history));
        assert_eq!(said, "", "nothing was started");
        DestroyWindow(settings);
        DestroyWindow(history);
    }
}
