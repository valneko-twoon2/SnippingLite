//! 段階を踏む書き込み（設定ファイル、画像、履歴、録画、一時フォルダーの掃除）の共通部品。
//!
//! 書き込みは段階を 1 つ終えるたびに `step` を呼ぶ。製品のビルドでは何もしない。テストのビルドでは、
//! クラッシュ時の整合性のテストが子プロセスをその段階で止めて殺し、残ったファイルを確かめる
//! （`test_support::crash`）。

use std::path::Path;

/// 書き込みの 1 段階（`label`、対象は `path`）を終えた。
#[cfg(test)]
pub fn step(label: &str, path: &Path) {
    crate::test_support::crash::step(label, path);
}

/// 書き込みの 1 段階を終えた。製品のビルドでは何もしない。
#[cfg(not(test))]
#[inline(always)]
pub fn step(_label: &str, _path: &Path) {}
