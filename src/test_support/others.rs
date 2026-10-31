//! ほかのプロセスで動く Snipping Lite の窓の代わり。子プロセスが、トレイと同じクラスの窓と、
//! 設定・履歴・プレイヤーの印を付けた題名「Snipping Lite」の窓を、隠したまま作って待つ。同時に
//! 動くほかのテストのプロセスや、利用者が動かしている Snipping Lite と同じ形の窓なので、窓を
//! 探す処理が、ほかのプロセスの窓をどう扱うかを確かめられる。

use super::{
    child::{self, Child},
    crash::test_name,
    stub_window, Sandbox,
};
use std::{
    ptr::{null, null_mut},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};

/// 子が窓を作り終えるのを待つ間隔と回数（60 秒）。テストのバイナリーの起動を含む。
const POLL: Duration = Duration::from_millis(10);
const POLLS: usize = 6000;
/// 子が殺されないまま待つ回数（120 秒）。過ぎたら自分で終わる。
const WAITS: usize = 2400;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// 窓を開いたほかのプロセス。落とすと殺して待つ。
pub struct Others {
    child: Child,
    /// 子の窓だけに付けた印。ほかのどのプロセスの窓とも重ならない。
    pub identity: String,
}

impl Others {
    /// 子プロセスに窓を作らせ、このプロセスから見つかるようになるまで待つ。子の出力は
    /// `sandbox` の中に書く。
    pub fn open(sandbox: &Sandbox) -> Others {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let identity = format!(
            "SnippingLite.Test.Other.{}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let name = test_name(module_path!(), "child_holds_the_windows_of_another_copy");
        let mut child = Child::start(sandbox, &name, &identity);
        let process = child.id();
        for _ in 0..POLLS {
            // SAFETY: plain window queries.
            if !unsafe { crate::ui::find_app_window(&identity, Some(process)) }.is_null() {
                return Others { child, identity };
            }
            if let Some(status) = child.wait(POLL) {
                panic!("the other process ended ({status}):\n{}", child.said());
            }
        }
        panic!("the other process opened no window:\n{}", child.said());
    }

    /// 窓を持つプロセスの ID。
    pub fn id(&self) -> u32 {
        self.child.id()
    }
}

#[test]
#[ignore = "Runs only as the child process of a test"]
fn child_holds_the_windows_of_another_copy() {
    let Some(identity) = child::value() else {
        return;
    };
    // SAFETY: hidden windows of this thread; they go away with the process.
    unsafe {
        let class = wide(crate::MAIN);
        let spec = WNDCLASSW {
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: GetModuleHandleW(null()),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        assert_ne!(RegisterClassW(&spec), 0, "the tray class");
        let tray = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            class.as_ptr(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        assert!(!tray.is_null(), "the tray window");
        let document = stub_window(crate::SETTINGS_WINDOW);
        // 親が待つ印は最後に付ける。見つかったときには、ほかの印も窓もそろっている。
        for name in crate::WINDOWS.iter().copied().chain([identity.as_str()]) {
            SetPropW(document, wide(name).as_ptr(), 1usize as _);
        }
        // 殺されるまで、窓へのメッセージに答えながら待つ。
        for _ in 0..WAITS {
            let mut message: MSG = std::mem::zeroed();
            for _ in 0..super::desk::MESSAGE_LIMIT {
                if PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) == 0 {
                    break;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
