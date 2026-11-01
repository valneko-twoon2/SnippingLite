//! テストのスレッドだけが使う、見えないデスクトップ。
//!
//! 入力デスクトップ（利用者の画面）では、ほかのプロセスが前面のウィンドウを替えたりキーを
//! 押したりすると、Windows がそこにあるスレッドのキーの状態に同期を保留し、そのスレッドが次に
//! GetKeyState を呼んだときに、表をまるごとシステムの状態で置き換える。テストが
//! SetKeyboardState で書いた Ctrl も消える。並列のテストの実行では、同時に動くほかのテストの
//! プロセスが窓を前に出すたびに起き、30 秒に 100 回を超えた。別のデスクトップのスレッドには
//! この同期が届かず、同じ間に 1 回も起きなかった。デスクトップは作るだけで切り替えないので、
//! 画面には何も出ない。

use std::{
    ffi::c_void,
    io::Write as _,
    ptr::null,
    sync::atomic::{AtomicUsize, Ordering},
};

type Desktop = *mut c_void;

/// デスクトップのすべての権利（GENERIC_ALL）。作った本人だけが使う。
const ALL_ACCESS: u32 = 0x1000_0000;

#[link(name = "user32")]
extern "system" {
    fn CreateDesktopW(
        name: *const u16,
        device: *const u16,
        mode: *const c_void,
        flags: u32,
        access: u32,
        attributes: *const c_void,
    ) -> Desktop;
    fn GetThreadDesktop(thread: u32) -> Desktop;
    fn SetThreadDesktop(desktop: Desktop) -> i32;
    fn CloseDesktop(desktop: Desktop) -> i32;
}

extern "system" {
    fn GetCurrentThreadId() -> u32;
}

/// このスレッドを移した先のデスクトップ。落とすと元のデスクトップへ戻して閉じる。
pub(super) struct Own {
    desktop: Desktop,
    previous: Desktop,
}

impl Own {
    /// 新しいデスクトップを作り、このスレッドを移す。窓やフックを持つスレッドは移れないので、
    /// 窓を作る前に呼ぶ。
    pub(super) fn enter() -> Own {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name: Vec<u16> = format!(
            "SnippingLite.Test.{}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
        .encode_utf16()
        .chain(Some(0))
        .collect();
        // SAFETY: a fresh desktop of this test; the thread has no window or hook yet.
        unsafe {
            let previous = GetThreadDesktop(GetCurrentThreadId());
            let desktop = CreateDesktopW(name.as_ptr(), null(), null(), 0, ALL_ACCESS, null());
            assert!(!desktop.is_null(), "a desktop of this test");
            if SetThreadDesktop(desktop) == 0 {
                CloseDesktop(desktop);
                panic!("the test thread moves to its own desktop before it makes a window");
            }
            Own { desktop, previous }
        }
    }
}

impl Drop for Own {
    fn drop(&mut self) {
        // SAFETY: both handles were taken by `enter` on this thread.
        unsafe {
            // 窓が残っていて戻れなければ、使い中のデスクトップは閉じられない。プロセスが終わる
            // ときに閉じる（後片付けの中では panic しない）。
            if SetThreadDesktop(self.previous) != 0 {
                CloseDesktop(self.desktop);
            } else {
                let _ = writeln!(
                    std::io::stderr(),
                    "Desk: the thread kept a window, so it stays on the desktop of its test"
                );
            }
        }
    }
}
