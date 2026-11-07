//! 切り取りのオーバーレイとツールバーを画面に出す。

use super::*;

/// オーバーレイを cloak したまま出して描き、描き終えてから見せる。最初に見える画面が欠けたり、
/// 前の撮影の画像が出たりしない。
///
/// ツールバーはオーバーレイを持ち主にした窓で、DWM は持ち主を cloak すると持たれた窓も一緒に
/// cloak する。撮影のあとも残したツールバーがもう出ているときは、cloak している間だけ持ち主の
/// 関係を外し、オーバーレイの上に置いてから cloak を解いて、関係を戻す。外さないと、撮影を
/// 始めるたびにツールバーが一瞬消えて出し直されたように見える。
pub(super) unsafe fn show_overlay(overlay: HWND, toolbar: HWND) {
    let kept = IsWindowVisible(toolbar) != 0;
    if kept {
        SetWindowLongPtrW(toolbar, GWLP_HWNDPARENT, 0);
    }
    ui::cloak(overlay, true);
    ShowWindow(overlay, SW_SHOW);
    UpdateWindow(overlay);
    if kept {
        // 前面に出したオーバーレイより上へ、見えないうちに戻しておく。
        SetForegroundWindow(overlay);
        SetFocus(overlay);
        ui::raise(toolbar);
        ui::cloak(overlay, false);
        SetWindowLongPtrW(toolbar, GWLP_HWNDPARENT, overlay as isize);
        return;
    }
    ShowWindow(toolbar, SW_SHOWNOACTIVATE);
    UpdateWindow(toolbar);
    ui::cloak(overlay, false);
    SetForegroundWindow(overlay);
    SetFocus(overlay);
}
