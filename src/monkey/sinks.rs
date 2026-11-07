//! 届いた画像を、届け先（履歴、保存先のフォルダー、クリップボード、通知）から数える。
//! 確定になりうるメッセージの前後で届け先を比べ、確定の操作の数と届いた画像の数が合うことを
//! 1 通ずつ確かめる。ほかのメッセージの後は、どの届け先も変わっていないことを確かめる。

use super::*;
use crate::test_support::clipboard_image;
use windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber;

/// 録画の範囲の下限（record::MIN_SIZE）。これより小さい範囲では録画を始めず、選び直せる。
const RECORD_MIN: i32 = crate::record::MIN_SIZE;

/// 届け先のその時の様子。
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Sinks {
    /// 履歴のいちばん新しい項目（ミリ秒）。古い項目は上限で消えるので、数ではなくこれで比べる。
    pub newest: u128,
    /// 保存先のフォルダーのファイルの数。
    pub shots: usize,
    /// クリップボードが変わるたびに増える番号。
    pub clipboard: u32,
    /// 出ている通知のウィンドウ（なければ 0）。
    pub toast: usize,
}

impl Sinks {
    pub fn observe(shots: &Path) -> Sinks {
        let files = std::fs::read_dir(shots).map_or(0, |list| list.flatten().count());
        Sinks {
            newest: history::list().first().map_or(0, |entry| entry.id),
            shots: files,
            // SAFETY: a plain query.
            clipboard: unsafe { GetClipboardSequenceNumber() },
            // SAFETY: the state belongs to this thread.
            toast: unsafe { toast::probe::look() }.map_or(0, |t| t.hwnd as usize),
        }
    }

    /// 前の様子から画像が 1 枚も届いていない（通知は消えてもよいが、新しく出てはいない）。
    pub fn assert_unchanged(&self, before: &Sinks, what: &str) {
        assert_eq!(
            self.newest, before.newest,
            "{what}: nothing goes into the history"
        );
        assert_eq!(self.shots, before.shots, "{what}: nothing is saved");
        assert_eq!(
            self.clipboard, before.clipboard,
            "{what}: nothing is copied"
        );
        assert!(
            self.toast == 0 || self.toast == before.toast,
            "{what}: no new notification"
        );
    }
}

/// 確定になりうるメッセージが、仕様どおりなら起こすこと。
#[derive(Clone, Copy, Debug)]
pub(super) enum Expect {
    /// 何も確定しない。
    Nothing,
    /// 範囲を選び終える（大きさが分かれば、その大きさ）。スクリーンショットなら、書き込みが
    /// オンのときは書き込みを始め、オフなら届ける。録画ならカウントダウンを始める。
    Selection(Option<(i32, i32)>),
    /// 書き込み中の切り取りを、書き込みを重ねて届ける。2 つ目は、Ctrl+C のように自動コピーの
    /// 設定にかかわらずクリップボードへ置く確定か（docs/behavior.md）。
    Markup((i32, i32), bool),
}

/// メッセージを送る前の、届け方を決める設定。
#[derive(Clone, Copy)]
pub(super) struct Flags {
    copy: bool,
    save: bool,
    history: bool,
    notify: bool,
    video: bool,
    markup: bool,
}

impl Flags {
    pub fn now() -> Flags {
        let st = s();
        Flags {
            copy: st.copy,
            save: st.save,
            history: st.history,
            notify: st.notify,
            video: st.video,
            markup: st.markup,
        }
    }
}

/// PNG の幅と高さ（IHDR）。
fn png_size(path: &Path) -> Option<(i32, i32)> {
    let bytes = std::fs::read(path).ok()?;
    let field = |at: usize| -> Option<i32> {
        Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as i32)
    };
    Some((field(16)?, field(20)?))
}

/// `expect` のとおりに届いたか（届いていないか）を確かめ、届いた画像の数を返す。
pub(super) fn verify(
    expect: Expect,
    flags: Flags,
    before: &Sinks,
    shots: &Path,
    what: &str,
) -> usize {
    let after = Sinks::observe(shots);
    let st = s();
    // SAFETY: the state belongs to this thread.
    let session = unsafe { markup::probe::look() };
    let delivered = match expect {
        Expect::Nothing => None,
        Expect::Selection(size) if flags.video => {
            // 録画の範囲: 小さすぎれば選び直せ、そうでなければカウントダウンが始まる。
            if let Some((width, height)) = size {
                let counts = width >= RECORD_MIN && height >= RECORD_MIN;
                assert_eq!(
                    !st.count_window.is_null(),
                    counts,
                    "{what}: a {width}x{height} recording area starts the countdown only from {RECORD_MIN}"
                );
                assert_eq!(
                    st.active, !counts,
                    "{what}: the overlay closes for the countdown"
                );
            }
            None
        }
        // 全画面はペンのボタンにかかわらず、書き込まずに届く。
        Expect::Selection(size) if flags.markup && st.mode != MODE_SCREEN => {
            let session = session.unwrap_or_else(|| panic!("{what}: the selection is marked up"));
            assert!(st.active, "{what}: marking up goes on in the overlay");
            if let Some(size) = size {
                assert_eq!(
                    (session.width, session.height),
                    size,
                    "{what}: the marked up snip"
                );
            }
            None
        }
        Expect::Selection(size) => Some(size),
        Expect::Markup(size, _) => Some(Some(size)),
    };
    let copies = flags.copy || matches!(expect, Expect::Markup(_, true));
    let Some(size) = delivered else {
        after.assert_unchanged(before, what);
        return 0;
    };
    assert!(
        !st.active && session.is_none(),
        "{what}: the snip ends once delivered"
    );
    let entries: Vec<_> = history::list()
        .into_iter()
        .filter(|entry| entry.id > before.newest)
        .collect();
    assert_eq!(
        entries.len(),
        flags.history as usize,
        "{what}: the history gets the snip exactly when it is on"
    );
    if let (Some(entry), Some(size)) = (entries.first(), size) {
        assert_eq!(
            png_size(&entry.image),
            Some(size),
            "{what}: the image is the selection"
        );
    }
    assert_eq!(
        after.shots,
        before.shots + flags.save as usize,
        "{what}: one file is saved exactly when saving is on"
    );
    assert_eq!(
        after.clipboard != before.clipboard,
        copies,
        "{what}: the clipboard changes exactly when copying is on or forced"
    );
    if copies {
        // クリップボードを見張るアプリ（クリップボードの履歴など）は、変わるたびに少しの間
        // クリップボードを開くので、読めるまで何度か試す。
        let (width, height, _) = (0..50)
            .find_map(|_| {
                clipboard_image().or_else(|| {
                    std::thread::sleep(Duration::from_millis(10));
                    None
                })
            })
            .unwrap_or_else(|| panic!("{what}: an image on the clipboard"));
        if let Some(size) = size {
            assert_eq!(
                (width, height),
                size,
                "{what}: the copied image is the selection"
            );
        }
    }
    let announced = flags.notify && (copies || flags.save);
    assert_eq!(
        after.toast != 0 && after.toast != before.toast,
        announced,
        "{what}: a notification follows exactly when it is on and the snip went somewhere"
    );
    1
}
