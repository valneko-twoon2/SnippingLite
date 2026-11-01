//! テスト用の窓口: 出ている通知の状態を、toast の外のテスト（モンキーテスト）から読めるようにする。
//! 読むほかには、滑らせる時計を進めることだけをする（`age_slide`）。

use super::*;
use std::time::Duration;

/// 出ている通知のうち、外のテストが確かめる部分の写し。
#[derive(Clone)]
pub(crate) struct Look {
    pub hwnd: HWND,
    pub scale: f32,
    /// 2 行目（保存したファイルの名前など）がある。ボタンの位置が下がる。
    pub body: bool,
    /// クリックで履歴の編集画面を開く。
    pub edit: bool,
    pub path: Option<PathBuf>,
    pub closing: bool,
    pub menu_hover: bool,
}

/// 通知が出ていれば、その写し。
pub(crate) unsafe fn look() -> Option<Look> {
    state().toast.as_deref().map(|toast| Look {
        hwnd: toast.hwnd,
        scale: toast.scale,
        body: !toast.body.is_empty(),
        edit: toast.edit,
        path: toast.path.clone(),
        closing: toast.closing,
        menu_hover: toast.menu_hover,
    })
}

/// 出ている通知が、滑り始めて（出始めて、または閉じ始めて）から `elapsed` たったことにする。
/// モンキーテストは本物のタイマーを流さず、滑らせるタイマーを自分で送るので、次の刻みでどこまで
/// 滑るかを本物の時計ではなく手で決める。
pub(crate) unsafe fn age_slide(elapsed: Duration) {
    if let Some(toast) = state().toast.as_deref_mut() {
        if let Some(started) = Instant::now().checked_sub(elapsed) {
            toast.started = started;
        }
    }
}

/// 表示を終えて閉じ始めるタイマーと、滑らせるタイマー。
pub(crate) const DISMISS_TIMER: usize = DISMISS;
pub(crate) const ANIMATION_TIMER: usize = ANIMATION;
/// 滑り終えるまでの時間。
pub(crate) const SLIDE: Duration = Duration::from_millis(SLIDE_MS as u64);
