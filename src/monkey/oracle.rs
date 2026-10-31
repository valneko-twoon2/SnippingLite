//! 1 手ごとの不変条件と、操作ごとに仕様が決める結果。期待値は README とコードの意図（コメント）
//! から決める。
//!
//! 不変条件（`invariants`）:
//! 1. 撮影中（`active`）であることと、オーバーレイが見えていることは同値。
//! 2. 書き込みのセッションがあれば撮影中。
//! 3. 書き込みのバーが見えていることと、セッションがあることは同値。
//! 4. ツールバーが見えているのは、書き込み中でない撮影中か、残す設定で開いたまま待機している
//!    ときに限り、そのときは必ず見えている。開いたままなのは、残す設定のときだけ。
//! 5. マウスをつかむのは、範囲を選んでいる・線を描いている・消しゴムでなぞっている間だけ。
//!    ボタンを離した後には、どれも残らない。撮影していなければつかまない（`GetCapture`）。
//! 6. 待機に戻った後は、選んでいる範囲、窓の強調、画面の写しが残らない。
//! 7. 録画は始まらず、カウントダウンは 3 から 1 の間で、撮影と同時には出ない。
//! 8. 撮影中は通知を出さない。出ている通知のウィンドウは生きている。
//! 9. 待機中は、撮影のために隠した自分のウィンドウをすべて戻している。
//! 10. モード、画像と録画、書き込みのオン・オフは、それを選ぶ操作でだけ変わり、書き込みの
//!     オン・オフは保存してある。外した設定（遅延、フレームレート）は初期値のまま。
//! 11. ダイアログ（失敗の知らせ）は出ず、メニューは選んだキーで閉じる。シェルに開かせるのは
//!     テストのフォルダーの中のものだけ。
//! 12. 届いた画像の数は確定の操作の数と合う（sinks.rs）。それ以外の手では何も届かない。
//! 13. 待機に戻るたびの GDI と USER のオブジェクトは、上限を超えて増えない。

use super::actions::{Action, Spot, Sys};
use super::sinks::Sinks;
use super::*;

mod contract;
use contract::contract;

/// 不変条件 1〜10。
pub(super) fn invariants(world: &World) {
    let st = s();
    // SAFETY: the state belongs to this thread.
    let session = unsafe { markup::probe::look() };
    // SAFETY: plain query on this thread.
    let capture = unsafe { GetCapture() };
    let count = !st.count_window.is_null();
    assert_eq!(
        st.active,
        shown(st.overlay),
        "snipping exactly while the overlay is shown"
    );
    assert!(
        session.is_none() || st.active,
        "marking up only while snipping"
    );
    assert_eq!(
        shown(st.markup_bar),
        session.is_some(),
        "the markup bar is shown exactly while marking up"
    );
    let kept = !st.active && st.toolbar_open && !count && st.recording.is_none() && !st.pending;
    assert_eq!(
        shown(st.toolbar),
        (st.active && session.is_none()) || kept,
        "the toolbar is shown while snipping, or kept between snips"
    );
    assert!(
        !st.toolbar_open || st.keep_toolbar,
        "only a kept toolbar stays open"
    );
    let drawing = session.is_some_and(|look| look.drawing || look.erasing);
    if capture == st.overlay && !capture.is_null() {
        assert!(
            st.active && (st.dragging || drawing),
            "the overlay holds the mouse only while something is being dragged"
        );
    }
    if !world.held {
        assert!(
            !st.dragging && !drawing,
            "nothing is dragged once the button is up"
        );
        assert!(
            capture != st.overlay,
            "the mouse is let go once the button is up"
        );
    }
    if !st.active {
        assert!(
            capture != st.overlay,
            "an idle overlay never holds the mouse"
        );
        assert!(
            !st.dragging && st.points.is_empty(),
            "no selection is left over"
        );
        assert!(
            st.hover_window.is_none() && st.hover_hwnd.is_null(),
            "no window highlight is left over"
        );
        assert!(
            st.dim_canvas.is_none()
                && st.live.is_none()
                && st.screen.is_null()
                && st.screen_dc.is_null(),
            "no copy of the screen is kept between snips"
        );
    }
    assert!(
        st.recording.is_none() && st.record_window.is_null() && st.record_frame.is_null(),
        "no recording starts"
    );
    if count {
        // SAFETY: plain window query.
        assert!(
            unsafe { IsWindow(st.count_window) } != 0,
            "the countdown window is alive"
        );
        assert!(
            shown(st.count_window) && !st.active,
            "the countdown shows on its own"
        );
        assert!(
            (1..=3).contains(&st.count_left),
            "the countdown is at {}",
            st.count_left
        );
    } else {
        assert!(
            st.count_points.is_empty() && st.count_target.is_none(),
            "a finished countdown keeps nothing"
        );
    }
    // SAFETY: the state belongs to this thread.
    let toast = unsafe { toast::probe::look() };
    assert!(
        !st.active || toast.is_none(),
        "no notification while snipping"
    );
    if let Some(look) = toast {
        // SAFETY: plain window query.
        assert!(
            unsafe { IsWindow(look.hwnd) } != 0,
            "the notification window is alive"
        );
    }
    if !st.active && !count {
        assert!(
            st.hidden.is_empty(),
            "our hidden windows are back after the snip"
        );
    }
    assert_eq!(
        st.mode, world.mode,
        "the mode changes only from the area menu"
    );
    assert_eq!(
        st.video, world.video,
        "only the camera and video buttons switch"
    );
    assert_eq!(
        st.markup, world.markup,
        "only the pen button switches markup"
    );
    assert_eq!(
        config::load_markup(),
        world.markup,
        "the pen button is saved"
    );
    assert!(
        st.delay == 0 && !st.pending && st.record_fps == 60,
        "the removed settings keep their first values"
    );
}

/// 不変条件 11〜13 と、操作の結果。手を送った後に呼ぶ。
pub(super) fn after_step(world: &mut World, scene: &Scene, before: &Snapshot, action: &Action) {
    let reports = scene.sentry.take();
    assert!(reports.is_empty(), "{reports:?}");
    let opened = crate::test_support::take_shell_opened();
    for (target, _) in &opened {
        assert!(
            Path::new(target).starts_with(&world.sandbox),
            "only the test's own files are opened, not {target}"
        );
    }
    let now = Sinks::observe(&world.shots);
    now.assert_unchanged(&world.last, action.label());
    world.last = now;
    invariants(world);
    contract(action, before, scene, &opened);
    objects(world);
}

/// 待機に戻るたびに、GDI と USER のオブジェクトが最初の待機から `SLACK` を超えて増えていない。
/// 最初の待機は、画像を 1 枚届けた後（PNG の符号化とクリップボードの準備が済んだ後）に取る。
fn objects(world: &mut World) {
    let st = s();
    // SAFETY: the state belongs to this thread.
    let quiet = !st.active && st.count_window.is_null() && unsafe { !toast::visible() };
    if !quiet || world.held || world.delivered == 0 {
        return;
    }
    let now = gui_objects();
    let first = *world.objects.get_or_insert(now);
    assert!(
        now.0 <= first.0 + SLACK && now.1 <= first.1 + SLACK,
        "GDI {} -> {}, USER {} -> {} between idle moments",
        first.0,
        now.0,
        first.1,
        now.1
    );
}
