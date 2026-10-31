//! 設定ファイル（`settings.ini`、`pen.ini`、`markup.ini`）を書くときの一時ファイルと、その後片付け。
//!
//! 一時ファイルの名前は `<ファイル名>.<書くプロセスの ID>.tmp`（`settings.ini.1234.tmp`）。トレイ、
//! 設定のウィンドウ、履歴のウィンドウは別々のプロセスで同時に動き、同じフォルダーの設定ファイルを
//! 書く。トレイが 2 つにならないのは、起動したプロセスが動いているトレイの窓を探して譲るから
//! （`hand_over_to_running_tray`）だが、ウィンドウのプロセスはトレイと同時に動き、トレイが止まった
//! 後も残る。名前に書いたプロセスが入るので、別のプロセスが書いている途中の一時ファイルと、止まった
//! プロセスが残した一時ファイルを見分けられる。同じファイルを 2 つのプロセスが同時に保存しても、
//! 互いの一時ファイルを書き換えない。
//!
//! # 残った一時ファイルは使わずに捨てる
//!
//! 書き終えてから名前を変える前に止まると、一時ファイルが新しい正しい中身のまま残ることがある。
//! それでも中身は読まずに消し、最後の名前のファイル（前に保存した中身）を使う。
//!
//! - 書き終えたかどうかを中身から見分けられない。書いている途中で止まった一時ファイルは前半しかなく、
//!   読めば欠けたキーが既定値になり、利用者の設定を黙って戻してしまう。電源が落ちたときは、
//!   大きさだけ合って中身が 0 のこともある。
//! - 最後の名前のファイルは、名前の変更が一度に置き換えるので、いつも前に書き終えた中身を持つ。
//! - 残った一時ファイルは、最後の名前のファイルより古いことがある（止まった後で、別のプロセスが同じ
//!   ファイルを保存し直した）。読めば新しい設定を古い設定で戻してしまう。
//!
//! 失うのは、止まる直前の 1 回の保存（設定の 1 つの変更や、ペンの色）だけになる。
//!
//! # 前の版の一時ファイル
//!
//! 前の版は、どのプロセスも同じ `<名前>.tmp`（`settings.tmp`）に書いていた。この版はその名前に
//! 書かないので、そのままでは残り続ける。書いたプロセスは名前からわからないので、最後に書かれてから
//! `STALE` より経ったものだけを消す。書いてから名前を変えるまでは一瞬なので、前の版のプロセスが
//! まだ動いていても（更新は `install.ps1` が前の版を止めてから行うが、別の場所に置いた前の版を
//! 動かすことはできる）、書いている途中のものには触れない。

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// 前の版の一時ファイルを、書いている途中ではないとみなすまでの時間。
const STALE: Duration = Duration::from_secs(60);

/// このプロセスが `path` を書いている途中の名前。
pub(super) fn name(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{}.tmp", std::process::id()));
    PathBuf::from(name)
}

/// `path` の一時ファイルのうち、もう名前を変えられることのないものを消す。消すのは、
///
/// - `<ファイル名>.<プロセス ID>.tmp` で、そのプロセスがもう動いていないもの（`staging::alive`）。
///   このプロセスと、動いているほかのプロセスが書いている途中のものは残す。
/// - 前の版が書いた `<名前>.tmp` で、`STALE` より前から書かれていないもの。
///
/// だけで、同じフォルダーのほかのファイル（ほかの設定ファイルの一時ファイルも）には触れない。
pub(super) fn remove_leftovers(path: &Path) {
    let (Some(folder), Some(file), Some(stem)) = (
        path.parent(),
        path.file_name().and_then(|n| n.to_str()),
        path.file_stem().and_then(|n| n.to_str()),
    ) else {
        return;
    };
    // 前の版の名前（`settings.ini` なら `settings.tmp`）。
    let old = format!("{stem}.tmp");
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let left = match writer(name, file) {
            Some(id) => !crate::staging::alive(id),
            None => name == old && stale(&entry.path()),
        };
        if left {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// `<file>.<プロセス ID>.tmp` の名前から、書いたプロセスの ID を読む。ほかの名前なら `None`。
fn writer(name: &str, file: &str) -> Option<u32> {
    let id = name
        .strip_prefix(file)?
        .strip_prefix('.')?
        .strip_suffix(".tmp")?;
    crate::staging::process_id(id)
}

/// 最後に書かれてから `STALE` より経ったか。時刻を読めないときや、時計が戻って未来の時刻に
/// なっているときは、経っていないとみなして残す。
fn stale(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|time| SystemTime::now().duration_since(time).ok())
        .is_some_and(|age| age > STALE)
}
