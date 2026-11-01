//! ディスプレイの構成の変化（WM_DISPLAYCHANGE）に、撮影の各画面がどう応えるか。拡大率（DPI）の
//! 変化（WM_DPICHANGED）は、それを受け取った窓がそれぞれ扱う（下の表の右の列）。
//!
//! | 起きていること | 構成（モニターの位置・解像度・数・並び）が変わった | 拡大率が変わった |
//! |---|---|---|
//! | 範囲を選んでいる | 取り消す（凍結した画像も、オーバーレイが覆う範囲も古い）。残す設定のツールバーは画面の中へ戻して出す | 続ける。オーバーレイは物理画素で覆うので動かさない |
//! | 撮影中のツールバー | 上のとおり | 新しい倍率で作り直し、提案された矩形の左上へ |
//! | 書き込み中 | 続ける（描いた線を失わない）。バーを新しい構成の画面の中へ置き直す | バーを新しい倍率で、同じ規則で置き直す |
//! | 残したツールバー（待機中） | はみ出したら、いちばん近いモニターの中へ寄せ、そこの倍率で描く | 撮影中と同じ |
//! | カウントダウン | 範囲にかかるモニターが変わったら取り消す（窓の録画は続ける） | 新しい倍率で範囲の真ん中に |
//! | 録画中 | 範囲にかかるモニターが変わったら止めて保存する（窓の録画は続ける）。表示は置き直す | 表示を新しい倍率で上部中央に |
//! | 通知 | 新しい作業領域の右下へ | 新しい倍率で右下へ |
//!
//! WM_DISPLAYCHANGE はすべての最上位の窓に届く。いつもある（持ち主のない）オーバーレイが受け取り、
//! ここでまとめて扱う。届いたときは自分あてに知らせを積むだけにして（PostMessage）、システムの
//! 知らせの処理を抜けてから確かめる。録画を止めると書き終えるまで待ち、ダイアログを出すことも
//! あるため。積んだ知らせは後から来るマウスやキーより先に取り出されるので、古い画面のまま範囲を
//! 確定することはない。並びが前に確かめたときと同じなら何もしない（色数だけの変化や、同じ構成の
//! 知らせ）。

use super::*;
use std::cell::Cell;
use ui::monitors::{self, Monitor};

/// WM_DISPLAYCHANGE を受け取った後で構成を確かめる、オーバーレイあての知らせ。
pub(super) const SETTLE: u32 = WM_APP + 0x20;

thread_local! {
    /// 最後に確かめたモニターの並び。撮影を始めるときと、構成を確かめるたびに覚え直す。
    static LAYOUT: Cell<Option<Vec<Monitor>>> = const { Cell::new(None) };
}

/// 今の並びを覚える。撮影を始めるときに呼ぶ（オーバーレイはこの並びの仮想画面を覆う）。
pub(crate) unsafe fn remember_layout() {
    remember(monitors::current());
}

/// 並び `layout` を覚える。テストは偽物の並びを覚えさせ、構成が変わったことにする。
pub(crate) fn remember(layout: Vec<Monitor>) {
    LAYOUT.set(Some(layout));
}

/// WM_DISPLAYCHANGE: 確かめるのは、知らせの処理を抜けてからにする。
pub(super) unsafe fn display_changed(overlay: HWND) {
    PostMessageW(overlay, SETTLE, 0, 0);
}

/// 構成を確かめ、前と違えば各画面を新しい構成に合わせる（上の表）。
pub(super) unsafe fn settle() {
    let now = monitors::current();
    let Some(before) = LAYOUT.replace(Some(now.clone())) else {
        // まだ一度も撮影していない: 合わせる画面がない。
        return;
    };
    if monitors::same_layout(&before, &now) {
        return;
    }
    let s = state();
    if s.active {
        if s.markup_session.is_some() {
            markup::show_bar();
        } else {
            cancel();
        }
    } else if !s.count_window.is_null() {
        if s.count_target.is_none() && monitors::moved_under(&before, &now, s.count_area) {
            cancel_countdown();
        }
    } else if let Some(recording) = s.recording.as_ref() {
        let moved = !recording.follows_window()
            && recording
                .display_area()
                .is_some_and(|area| monitors::moved_under(&before, &now, area));
        if moved {
            stop_recording();
        } else {
            replace_indicator();
        }
    } else if s.toolbar_open && IsWindowVisible(s.toolbar) != 0 {
        show_kept_toolbar();
    }
    toast::display_changed();
}

#[cfg(test)]
mod tests;
