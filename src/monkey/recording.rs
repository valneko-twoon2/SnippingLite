//! 録画の確かめ。カウントダウンの最後の刻みで本物の録画が始まり、止める手（表示の停止のボタン、
//! もう一度の開始、トレイのメニューの「録画を停止」と TRAY_STOP）か、終了で終わる。期待値は
//! README から決める:
//! - 録画中は、範囲を囲む赤い枠と、経過時間と停止のボタンのある小さな表示が画面の上に出る。
//!   どちらも動画には写らない（画面の写しから外す）。
//! - 録画は書きかけの名前（`.partial`）で書き、仕上がってから最後の名前になる。
//! - 止めると、再生できる MP4（H.264 は幅と高さが偶数なので、範囲を偶数に切り下げた大きさ）が
//!   1 つできる。「元の画面録画を自動的に保存する」がオンなら録画の保存先に、オフなら一時
//!   フォルダーに残る（どちらになるかは録画を始めたときの設定で決まる）。「自動的に変更にコピー」が
//!   オンなら、そのファイルがクリップボードに置かれる。通知がオンなら知らせ、通知はそのファイルを
//!   開く。録画は履歴にもスクリーンショットの保存先にも入らない。
//! - 終了すると、録画をファイルに仕上げてから終わる（コピーも通知もしない）。

use super::*;
use crate::player::{Engine, Status};
use crate::test_support::clipboard_files;
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowDisplayAffinity;

/// 録画のフォルダーの中のファイル。
#[derive(Clone, Default, Debug, PartialEq)]
pub(super) struct Files {
    /// 仕上がった録画（`.mp4`）。
    pub done: Vec<PathBuf>,
    /// 書きかけ（`.partial`）。
    pub partial: Vec<PathBuf>,
}

impl Files {
    fn observe(folder: &Path) -> Files {
        let mut files = Files::default();
        for path in std::fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
        {
            match path.extension().and_then(|e| e.to_str()) {
                Some("mp4") => files.done.push(path),
                Some("partial") => files.partial.push(path),
                _ => {}
            }
        }
        files.done.sort();
        files
    }
}

/// 録画の保存先（Desk が設定に書く）と、保存しない録画を置く一時フォルダー（TMP の下）。
fn folders(world: &World) -> [PathBuf; 2] {
    [
        world.sandbox.join("videos"),
        world
            .sandbox
            .join("temp")
            .join("Snipping Lite")
            .join("Recordings"),
    ]
}

/// カウントダウンの最後の刻みで決まること。
pub(super) struct Started {
    /// 録画する範囲（画面の座標）。幅と高さは偶数に切り下げてある。
    area: RECT,
    /// 録画の保存先に残すか。
    saved: bool,
    /// 始める前の、保存先と一時フォルダーの録画。
    before: [Files; 2],
    /// 録画中の表示と枠。止めた後に消えたことを確かめる。
    indicator: HWND,
    frame: HWND,
}

/// 最後の刻みを送る前に、始まる録画がどうなるはずかを決める（表示と枠は、始まってから入れる）。
pub(super) fn expect_start(world: &World) -> Started {
    let st = s();
    let area = st.count_area;
    let [saved, temporary] = folders(world);
    Started {
        area: RECT {
            right: area.left + ((area.right - area.left) & !1),
            bottom: area.top + ((area.bottom - area.top) & !1),
            ..area
        },
        saved: st.save_video,
        before: [Files::observe(&saved), Files::observe(&temporary)],
        indicator: null_mut(),
        frame: null_mut(),
    }
}

/// 最後の刻みを送った後: 録画が始まっている。
pub(super) fn started(world: &mut World, expected: Started) {
    let st = s();
    assert!(
        st.count_window.is_null() && st.recording.is_some(),
        "3, 2, 1 ends in a recording"
    );
    world.recording = Some(Started {
        indicator: st.record_window,
        frame: st.record_frame,
        ..expected
    });
}

/// 録画中の不変条件（oracle の 7）。
pub(super) fn invariants(world: &World) {
    let st = s();
    let Some(started) = &world.recording else {
        assert!(
            st.recording.is_none(),
            "a recording starts only at the end of the countdown"
        );
        assert!(
            st.record_window.is_null() && st.record_frame.is_null(),
            "no recording indicator without a recording"
        );
        return;
    };
    assert!(
        st.recording.is_some(),
        "the recording goes on until stopped"
    );
    assert!(
        !st.active && st.count_window.is_null(),
        "nothing else is snipped while recording"
    );
    for (window, what) in [(st.record_window, "indicator"), (st.record_frame, "frame")] {
        assert!(shown(window), "the recording {what} is shown");
        let mut affinity = 0;
        // SAFETY: a plain window query.
        unsafe { GetWindowDisplayAffinity(window, &mut affinity) };
        assert_eq!(
            affinity, WDA_EXCLUDEFROMCAPTURE,
            "the recording {what} is not part of the video"
        );
    }
    // 枠は範囲のすぐ外を、どの辺も同じ太さで囲む。
    let (frame, area) = (window_rect(st.record_frame), started.area);
    let edges = [
        area.left - frame.left,
        area.top - frame.top,
        frame.right - area.right,
        frame.bottom - area.bottom,
    ];
    assert!(
        edges[0] >= 1 && edges.iter().all(|&edge| edge == edges[0]),
        "the frame outlines the recorded area: {edges:?}"
    );
    // 表示は範囲のあるモニターの上部中央（ツールバーと同じく上から 12 DIP）に出る。
    let indicator = window_rect(st.record_window);
    let center = POINT {
        x: (area.left + area.right) / 2,
        y: (area.top + area.bottom) / 2,
    };
    // SAFETY: plain monitor queries.
    let monitor = unsafe {
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(
            MonitorFromPoint(center, MONITOR_DEFAULTTONEAREST),
            &mut info,
        );
        info.rcMonitor
    };
    let width = indicator.right - indicator.left;
    assert_eq!(
        (indicator.left, indicator.top),
        (
            (monitor.left + monitor.right - width) / 2,
            monitor.top + (12.0 * st.scale).round() as i32
        ),
        "the recording indicator is at the top center of the screen"
    );
    // 録画中は書きかけの名前で書くので、仕上がった名前のファイルは増えない。
    let folder = &folders(world)[!started.saved as usize];
    assert_eq!(
        Files::observe(folder).done,
        started.before[!started.saved as usize].done,
        "a recording gets its final name only when it is finished"
    );
}

/// 止める前の、届け方を決める設定（止めたときの設定で決まる）。
#[derive(Clone, Copy)]
pub(super) struct Flags {
    copy: bool,
    notify: bool,
}

impl Flags {
    pub fn now() -> Flags {
        let st = s();
        Flags {
            copy: st.copy_video,
            notify: st.notify,
        }
    }
}

/// 録画が終わった後: 仕上がった MP4 が 1 つでき、設定どおりに届く。`exited` は終了で終わったとき。
pub(super) fn verify_stop(world: &mut World, flags: Flags, exited: bool, what: &str) {
    let started = world
        .recording
        .take()
        .unwrap_or_else(|| panic!("{what}: a recording that started at the end of the countdown"));
    let st = s();
    assert!(st.recording.is_none(), "{what} ends the recording");
    if !exited {
        // SAFETY: plain window queries.
        let gone = unsafe { IsWindow(started.indicator) == 0 && IsWindow(started.frame) == 0 };
        assert!(
            gone && st.record_window.is_null() && st.record_frame.is_null(),
            "{what}: the indicator and the frame go away with the recording"
        );
    }
    let now = folders(world).map(|folder| Files::observe(&folder));
    assert!(
        now.iter().all(|files| files.partial.is_empty()),
        "{what}: no unfinished file is left: {now:?}"
    );
    let (kept, other) = (!started.saved as usize, started.saved as usize);
    let new: Vec<&PathBuf> = now[kept]
        .done
        .iter()
        .filter(|path| !started.before[kept].done.contains(path))
        .collect();
    let place = ["the recording folder", "the temporary folder"][kept];
    assert_eq!(new.len(), 1, "{what}: one recording is kept in {place}");
    assert_eq!(
        now[other].done, started.before[other].done,
        "{what}: and nothing in the other folder"
    );
    let path = new[0].clone();
    let user = gui_objects().1;
    let size = playable(&path).unwrap_or_else(|error| panic!("{what}: {path:?}: {error}"));
    // 再生の部品は、閉じた後しばらく、作業用のスレッドの USER のオブジェクトを残す（6 回開いて
    // 測ると、0.5 秒後には 1 つを除いて消えている）。待機のオブジェクトの数に入れないよう、消える
    // まで待つ（消えなければ、ケースの後の数で見つかる）。
    let deadline = Instant::now() + Duration::from_secs(2);
    while gui_objects().1 > user + 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let area = started.area;
    assert_eq!(
        size,
        (
            (area.right - area.left) as usize,
            (area.bottom - area.top) as usize
        ),
        "{what}: the video is the recorded area"
    );
    let before = world.last;
    let after = Sinks::observe(&world.shots);
    let copies = flags.copy && !exited;
    assert_eq!(
        after.clipboard != before.clipboard,
        copies,
        "{what}: the file goes on the clipboard exactly when copying recordings is on"
    );
    if copies {
        assert_eq!(
            clipboard_files(),
            std::slice::from_ref(&path),
            "{what}: the copied file"
        );
    }
    let announced = flags.notify && !exited;
    assert_eq!(
        after.toast != 0 && after.toast != before.toast,
        announced,
        "{what}: a notification follows exactly when it is on"
    );
    if announced {
        // SAFETY: the state belongs to this thread.
        let look = unsafe { toast::probe::look() }.expect("the notification");
        assert_eq!(
            look.path.as_ref(),
            Some(&path),
            "{what}: it opens the video"
        );
    }
    assert_eq!(
        (after.newest, after.shots),
        (before.newest, before.shots),
        "{what}: a recording goes into neither the history nor the screenshots"
    );
    world.recorded += 1;
    world.last = after;
    // プロセスで初めての録画と再生は、Media Foundation、符号化と再生の部品が 1 度だけ作る GDI と
    // USER のオブジェクトを残す（同じトレイで 6 回録画して測ると、2 回目からは増えない）。待機の
    // オブジェクトの数は、その後の待機から数え直す。
    if !FINISHED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        world.objects = None;
    }
}

/// このプロセスで、録画が 1 度でも仕上がったか。
static FINISHED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 再生の部品で開いて、最初の画像を取り出せるか。取り出せたら、その幅と高さ。再生の部品は
/// COM のアパートメントを持つので、トレイのスレッドではなく別のスレッドで開く。
fn playable(path: &Path) -> Result<(usize, usize), String> {
    let path = path.to_path_buf();
    let open = std::thread::spawn(move || {
        let mut engine = Engine::open(&path).map_err(|e| format!("does not open: {e}"))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while engine.status() == Status::Opening && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if engine.status() != Status::Ready {
            return Err(format!("the player says {:?}", engine.status()));
        }
        if engine.duration().is_zero() {
            return Err("the video has no length".into());
        }
        while Instant::now() < deadline {
            if let Some(picture) = engine.frame().map_err(|e| format!("no frame: {e}"))? {
                return Ok((picture.width, picture.height));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err("no frame came".into())
    });
    open.join()
        .unwrap_or_else(|_| Err("the player panicked".into()))
}
