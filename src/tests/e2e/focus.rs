//! 入力を送る前に、前面の窓とポインターの下の窓が、このテストかテストするアプリのものかを確かめる。
//!
//! 合成した入力（`SendInput`）は前面の窓に届く。撮影の画面が閉じると Windows は次に前面に出す
//! 窓を選ぶが、ほかの作業が一瞬開いた端末の窓が閉じた後などは、それが背景の窓ではなく、ほかの
//! アプリ（最小化したタスク マネージャーなど）になることがある。そのまま入力を送ると、キーがそのアプリに
//! 入るか、そのアプリが管理者として動いていれば UIPI に捨てられ、ショートカットもクリックも
//! 届かないままテストが失敗する。そこで、ボタンやキーを押す前に、前面の窓がこのテストか
//! テストするアプリのプロセスのものでなければ背景の窓を前面に戻し、戻せなければほかのアプリに
//! 入力を送らずに panic する。
//!
//! 前面を取られるのが、メニューを開いている間やキーを続けて押している間だと、メニューが閉じて
//! キーが届かない。そこで、待つたびにほかのプロセスの窓が前面に出たかを数え（`watch_foreground`）、
//! テストの外からカーソルが動かされたことも数えて（`input.rs`）、それが起きた回のテストが
//! 失敗したときだけ、上限まで初めからやり直す（`on_desktop`）。

use super::*;

/// テストするアプリのプロセス（トレイと、テストが開いた履歴の窓）。
static PROCESSES: Mutex<Vec<u32>> = Mutex::new(Vec::new());
/// 前面に戻す背景の窓（なければ null）。前面に戻しても重なり順は変わらない（`backdrop.rs`）。
static BACKDROP_WINDOW: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(null_mut());
/// テストの外からの邪魔の回数。背景の窓がある間にほかのプロセスの窓が前面に出たことと、
/// テストの置いたカーソルが動かされていたこと。
static DISTURBANCES: AtomicUsize = AtomicUsize::new(0);
/// 最後に見た、ほかのプロセスの前面の窓（前面に出ている間、同じ窓を何度も数えない）。
static LAST_FOREIGN: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(null_mut());

/// 前面を取り戻すまでに試す回数。1 回ごとに 100 ms 待つので、最長でおよそ 3 秒。
const CLAIM_TRIES: usize = 30;
/// テストの外から邪魔された回に失敗したテストを、初めから実行し直す回数を含めた上限。
const ATTEMPTS: usize = 3;

/// 画面を 1 件のテストで占有し、背景の窓で覆ったうえで `body` を実行する。テストの外から邪魔された
/// 回（ほかのプロセスの窓が前面に出た、カーソルが動かされた）に失敗したら、上限まで初めから
/// やり直す。邪魔されないまま失敗したら、やり直さずにその失敗を返す。
pub fn on_desktop(body: impl Fn(&Backdrop)) {
    let _desktop = DESKTOP.lock().unwrap_or_else(|e| e.into_inner());
    for attempt in 1..=ATTEMPTS {
        let backdrop = Backdrop::new();
        let before = DISTURBANCES.load(Ordering::Relaxed);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(&backdrop)));
        // 失敗の後片付け（アプリを止める間の待ち）で見えた分も数える。
        let disturbed = DISTURBANCES.load(Ordering::Relaxed) - before;
        drop(backdrop);
        match result {
            Ok(()) => return,
            Err(panic) if disturbed == 0 || attempt == ATTEMPTS => {
                if disturbed > 0 {
                    eprintln!("{attempt} 回とも、テストの外から邪魔された");
                }
                std::panic::resume_unwind(panic);
            }
            Err(_) => eprintln!(
                "{attempt} 回目の失敗の間に、テストの外から {disturbed} 回邪魔されたので、初めからやり直す"
            ),
        }
    }
}

/// 前面の窓を見て、ほかのプロセスの窓が新たに前面に出ていたら数える。`settle` が待つたびに呼ぶ。
/// 窓が切り替わる途中の、前面の窓がない瞬間は数えない。
pub fn watch_foreground() {
    if BACKDROP_WINDOW.load(Ordering::Relaxed).is_null() {
        return;
    }
    // SAFETY: a plain window query.
    let window = unsafe { GetForegroundWindow() };
    if window.is_null() {
        return;
    }
    if ours(window) {
        LAST_FOREIGN.store(null_mut(), Ordering::Relaxed);
    } else if LAST_FOREIGN.swap(window, Ordering::Relaxed) != window {
        note_disturbance();
    }
}

/// テストの外からの邪魔を 1 回数える。
pub fn note_disturbance() {
    DISTURBANCES.fetch_add(1, Ordering::Relaxed);
}

/// `process` をテストするアプリのプロセスとして数える。
pub fn adopt(process: u32) {
    PROCESSES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(process);
}

/// 終わったプロセスを数えるのをやめる。
pub fn disown(process: u32) {
    PROCESSES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|&p| p != process);
}

/// 前面を取り戻すときに前に出す背景の窓（null で解除）。
pub fn set_backdrop_window(hwnd: HWND) {
    BACKDROP_WINDOW.store(hwnd, Ordering::Relaxed);
}

fn process_of(window: HWND) -> u32 {
    let mut process = 0;
    // SAFETY: a plain window query; a stale handle only gives 0.
    unsafe { GetWindowThreadProcessId(window, &mut process) };
    process
}

/// `window`（またはそれを含む最上位の窓）がこのテストかテストするアプリのプロセスのものか。
fn ours(window: HWND) -> bool {
    if window.is_null() {
        return false;
    }
    let process = process_of(window);
    process == std::process::id()
        || PROCESSES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&process)
}

/// 失敗の知らせに書く、窓のクラス名とプロセス。
fn describe(window: HWND) -> String {
    let mut class = [0u16; 128];
    // SAFETY: the buffer length is passed along.
    let length = unsafe { GetClassNameW(window, class.as_mut_ptr(), class.len() as i32) };
    format!(
        "class {:?}, process {}",
        String::from_utf16_lossy(&class[..length.max(0) as usize]),
        process_of(window)
    )
}

/// 前面の窓がほかのプロセスのものなら、背景の窓を前面に戻す。戻せなければ、その窓を書いた
/// 知らせを返す。キーやボタンを押す前と離す前に呼ぶ（`claim_foreground` と `input.rs`）。
pub fn reclaim_foreground() -> Result<(), String> {
    for attempt in 0..CLAIM_TRIES {
        // SAFETY: a plain window query.
        let window = unsafe { GetForegroundWindow() };
        if ours(window) {
            return Ok(());
        }
        if attempt == 0 {
            eprintln!(
                "前面の窓がほかのプロセスのもの（{}）なので、背景の窓を前面に戻す",
                describe(window)
            );
        }
        let backdrop = BACKDROP_WINDOW.load(Ordering::Relaxed);
        if !backdrop.is_null() {
            // SAFETY: the backdrop belongs to this thread; activation only sends it messages.
            unsafe { SetForegroundWindow(backdrop) };
        }
        settle(100);
    }
    // SAFETY: a plain window query.
    let window = unsafe { GetForegroundWindow() };
    Err(format!(
        "前面の窓（{}）がこのテストのものではなく、取り戻せない。入力を送らずに止める",
        describe(window)
    ))
}

/// 押す前に前面を確かめ、取り戻せなければ panic する（ほかのアプリに入力を送らない）。
/// 何も押していないときだけ呼ぶ。押したままのキーやボタンを残して panic しないよう、
/// 押している間は `reclaim_foreground` を使い、離してから止める。
pub fn claim_foreground() {
    if let Err(message) = reclaim_foreground() {
        panic!("{message}");
    }
}

/// ポインターの下の窓がこのテストかテストするアプリのものか確かめる。ほかのアプリの窓
/// （背景の窓より上にある最前面の窓）をクリックしないよう、ボタンを押す前に呼ぶ。
pub fn assert_ours_under_cursor() {
    let mut p = POINT { x: 0, y: 0 };
    // SAFETY: plain cursor and window queries.
    let window = unsafe {
        GetCursorPos(&mut p);
        WindowFromPoint(p)
    };
    assert!(
        ours(window),
        "{},{} の窓（{}）はこのテストのものではないので、押さずに止める",
        p.x,
        p.y,
        describe(window)
    );
}
