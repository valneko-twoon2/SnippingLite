//! 操作ごとに、仕様が決める結果。

use super::*;

fn top_left(hwnd: HWND) -> (i32, i32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: plain window query.
    unsafe { GetWindowRect(hwnd, &mut rect) };
    (rect.left, rect.top)
}

/// 新しく開いたツールバーはマウスのあるモニターの上部中央に、残したツールバーは置いた所に出る。
fn assert_toolbar_placed(before: &Snapshot) {
    let st = s();
    let at = top_left(st.toolbar);
    if before.toolbar_open {
        let was = (before.toolbar_rect.left, before.toolbar_rect.top);
        assert_eq!(at, was, "a kept toolbar comes back where it was left");
        return;
    }
    // SAFETY: plain cursor and monitor queries.
    let monitor = unsafe {
        let mut cursor = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(
            MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST),
            &mut info,
        );
        info.rcMonitor
    };
    // README: 次に開くと上部中央に戻る（幅 376 DIP、上から 12 DIP）。
    let width = (376.0 * st.scale).round() as i32;
    let expected = (
        (monitor.left + monitor.right - width) / 2,
        monitor.top + (12.0 * st.scale).round() as i32,
    );
    assert_eq!(
        at, expected,
        "a newly opened toolbar starts at the top center"
    );
}

fn closing_or_gone() -> bool {
    // SAFETY: the state belongs to this thread.
    unsafe { toast::probe::look() }.is_none_or(|look| look.closing)
}

/// 操作ごとの結果。届く所に届いた操作だけを確かめる（届かなかったものは何も変えない）。
pub(super) fn contract(
    action: &Action,
    before: &Snapshot,
    scene: &Scene,
    opened: &[(String, Option<String>)],
) {
    let st = s();
    // SAFETY: the state belongs to this thread.
    let session = unsafe { markup::probe::look() };
    let about = || {
        opened
            .iter()
            .any(|(target, _)| target.ends_with("about.html"))
    };
    // README: 録画を止めるのは、表示の停止のボタン、もう一度の開始（ショートカット、トレイの
    // アイコン）、トレイのメニューの「録画を停止」（録画中だけ先頭に出る）だけ。
    if before.recording {
        let stops = matches!(
            action,
            Action::Start(_)
                | Action::RecordStop
                | Action::TrayMenu(Some(0))
                | Action::Command(WM_COMMAND, TRAY_STOP)
        );
        assert_eq!(st.recording.is_none(), stops, "{action:?} while recording");
    }
    // README: 残したツールバーは、× で閉じるまで移動した位置に出る（撮影を挟んでも）。
    if before.toolbar_open && st.toolbar_open && !matches!(action, Action::MoveToolbar(_)) {
        let was = (before.toolbar_rect.left, before.toolbar_rect.top);
        assert_eq!(
            top_left(st.toolbar),
            was,
            "a kept toolbar stays where it was moved"
        );
    }
    match action {
        Action::Start(_) if before.recording => {
            assert!(
                !st.active && st.count_window.is_null(),
                "a start event that stops a recording starts nothing"
            );
        }
        // 録画中のメニューは「録画を停止」（上で確かめた）、「Snipping Lite」、About、終了の順。
        Action::TrayMenu(Some(0)) if before.recording => {}
        Action::TrayMenu(Some(1)) if before.recording => {
            assert!(
                shown(scene.settings),
                "Snipping Lite in the menu opens the settings while recording"
            );
        }
        Action::Start(_) if before.count.is_some() => {
            assert!(
                st.count_window.is_null(),
                "a start event cancels the countdown"
            );
            assert!(!st.active, "a cancelled countdown starts no snip");
        }
        Action::Start(_) if before.active => {
            assert!(st.active, "a start event leaves a running snip alone");
            assert_eq!(
                session.is_some(),
                before.session.is_some(),
                "and its markup"
            );
            if before.dragging {
                assert!(st.dragging, "the selection goes on");
                assert_eq!(
                    (st.start.x, st.start.y, st.end.x, st.end.y, st.points.len()),
                    (
                        before.start.x,
                        before.start.y,
                        before.end.x,
                        before.end.y,
                        before.points
                    ),
                    "the selection is the same"
                );
            }
        }
        Action::Start(start) => {
            assert!(st.active && session.is_none(), "{start:?} begins a snip");
            assert_toolbar_placed(before);
        }
        // 撮影中とカウントダウン中は、設定や履歴の画面を開かずに撮影の画面を前面へ戻し、
        // 撮影はそのまま続く（tray::bring_capture_forward）。前面とフォーカスは tray::tests が確かめる。
        Action::TrayMenu(Some(0)) | &Action::Command(WM_COMMAND, TRAY_SETTINGS | TRAY_HISTORY)
            if before.active || before.count.is_some() =>
        {
            assert_eq!(
                (st.active, st.count_window.is_null()),
                (before.active, before.count.is_none()),
                "asking for a window leaves the capture going"
            );
        }
        Action::TrayMenu(Some(0)) if !before.active => {
            assert!(
                shown(scene.settings),
                "Snipping Lite in the menu opens the settings"
            );
        }
        Action::TrayMenu(Some(1)) => assert!(about(), "About in the menu opens the page"),
        Action::TrayMenu(None) => {
            assert_eq!(st.active, before.active, "leaving the menu changes nothing");
        }
        &Action::Command(WM_COMMAND, TRAY_SETTINGS) if !before.active => {
            assert!(shown(scene.settings), "the settings open");
        }
        &Action::Command(WM_COMMAND, TRAY_HISTORY) if !before.active => {
            assert!(shown(scene.history), "the history opens");
        }
        &Action::Command(WM_COMMAND, TRAY_ABOUT) => assert!(about(), "the About page opens"),
        &Action::Command(WM_COMMAND, TRAY_STOP) | &Action::Command(WM_CLOSE, _) => {
            // 止めるのは録画だけ（上）。撮影とカウントダウンはそのまま、トレイは閉じない。
            assert_eq!(
                st.active, before.active,
                "nothing to stop, nothing to close"
            );
            assert_eq!(st.count_window.is_null(), before.count.is_none());
        }
        Action::Reload(settings) => {
            let got = (
                (st.copy, st.save, st.ask, st.history, st.countdown),
                (st.skip_title, st.record_cursor, st.freeze, st.notify),
                (
                    st.keep_toolbar,
                    st.outline,
                    st.outline_color,
                    st.outline_width,
                ),
                (
                    st.mic,
                    st.system_audio,
                    st.app_audio,
                    st.copy_video,
                    st.save_video,
                ),
            );
            let set = settings;
            let wanted = (
                (set.copy, set.save, set.ask, set.history, set.countdown),
                (set.skip_title, set.record_cursor, set.freeze, set.notify),
                (
                    set.keep_toolbar,
                    set.outline,
                    set.outline_color,
                    set.outline_width,
                ),
                (
                    set.mic,
                    set.system_audio,
                    set.app_audio,
                    set.copy_video,
                    set.save_video,
                ),
            );
            assert_eq!(got, wanted, "the running tray takes the new settings");
            assert!(
                set.keep_toolbar || !st.toolbar_open,
                "switching keeping off closes it"
            );
        }
        Action::Key(VK_ESCAPE, _) if before.overlay => {
            assert!(!st.active, "Escape ends the snip");
        }
        Action::Key(..)
        | Action::Move(_)
        | Action::Hold(_)
        | Action::Drag(_)
        | Action::Click(_)
        | Action::DoubleClick(_)
            if before.session.is_some() && before.overlay =>
        {
            // 書き込み中のマウスとキーは、Escape（上）と Enter・Ctrl+C のほかは書き込みを続ける。
            let ends = matches!(action, Action::Key(VK_RETURN, _) | Action::Key(0x43, true));
            assert_eq!(session.is_some(), !ends, "{action:?} while marking up");
        }
        Action::Key(..) if before.overlay => {
            assert!(
                st.active,
                "keys other than Escape do not end a snip before markup"
            );
        }
        &Action::Toolbar(button) if before.toolbar.is_some() => toolbar(button, before, scene),
        Action::AreaMenu(Some(position)) if before.toolbar.is_some() => {
            let mode = area_mode(*position);
            assert_eq!(st.mode, mode, "the menu sets the mode");
            if mode != MODE_SCREEN {
                assert!(st.active, "choosing a mode snips in it");
            }
            if !before.active && st.active {
                assert_toolbar_placed(before);
            }
        }
        Action::AreaMenu(None) if before.toolbar.is_some() => {
            assert_eq!(st.active, before.active, "leaving the menu changes nothing");
        }
        &Action::MoveToolbar(at) if before.toolbar.is_some() => {
            assert_eq!(
                top_left(st.toolbar),
                at,
                "the toolbar is where it was dragged"
            );
            assert_eq!(
                st.active, before.active,
                "moving the toolbar keeps the snip"
            );
        }
        Action::ToolbarSlip(..) | Action::BarSlip(..) => {
            assert_eq!(
                st.active, before.active,
                "letting go on another button is no click"
            );
            assert_eq!(session.map(|l| l.marks), before.session.map(|l| l.marks));
        }
        &Action::Bar(button) if before.bar => {
            let tool = match button {
                markup::probe::PEN => Some(markup::Tool::Pen),
                markup::probe::HIGHLIGHTER => Some(markup::Tool::Highlighter),
                markup::probe::ERASER => Some(markup::Tool::Eraser),
                _ => None,
            };
            match button {
                markup::probe::CLOSE => assert!(!st.active, "the × throws the snip away"),
                markup::probe::CAPTURE => {}
                _ => {
                    let look = session.expect("the other buttons keep marking up");
                    if let Some(tool) = tool {
                        assert_eq!(look.tool, tool, "the bar picks the tool");
                    }
                }
            }
        }
        Action::ToastClick(spot) if before.toast.is_some() => {
            toast_click(*spot, before, scene, opened)
        }
        Action::ToastDismiss if before.toast.is_some() => {
            assert!(
                closing_or_gone(),
                "the dismiss timer closes the notification"
            );
        }
        // 閉じ始めた通知は、滑り終えたところで消える。出てくる通知は、滑り終えても残る。
        &Action::ToastSlide(ms) if before.toast.is_some() => {
            let closing = before.toast.as_ref().is_some_and(|look| look.closing);
            let over = Duration::from_millis(ms) >= toast::probe::SLIDE;
            // SAFETY: the state belongs to this thread.
            let gone = unsafe { toast::probe::look() }.is_none();
            assert_eq!(
                gone,
                closing && over,
                "a notification goes away once it has slid out"
            );
        }
        Action::ToastMenu(choice) if before.toast.is_some() => {
            if *choice == Some(0) {
                assert!(shown(scene.settings), "Settings in the menu opens them");
            }
            if choice.is_some() {
                assert!(
                    closing_or_gone(),
                    "a choice in the menu closes the notification"
                );
            } else if before.toast.as_ref().is_some_and(|look| !look.closing) {
                // 指している間は消えるタイマーが止まるので、何も選ばなければ出たまま。
                assert!(
                    !closing_or_gone(),
                    "leaving the menu keeps the notification"
                );
            }
        }
        Action::Tick => match before.count {
            Some(1) => assert!(
                st.count_window.is_null() && st.recording.is_some(),
                "the countdown ends in a recording"
            ),
            Some(left) => assert_eq!(st.count_left, left - 1, "the countdown counts down"),
            None => {}
        },
        Action::CountClick if before.count.is_some() => {
            assert!(
                st.count_window.is_null() && !st.active,
                "a click cancels the countdown"
            );
        }
        Action::System(which) => {
            assert_eq!(st.active, before.active, "{which:?} keeps the snip");
            assert_eq!(
                st.recording.is_some(),
                before.recording,
                "{which:?} keeps the recording"
            );
            assert_eq!(
                session.is_some(),
                before.session.is_some(),
                "{which:?} keeps the markup"
            );
            assert_eq!(st.count_window.is_null(), before.count.is_none());
            if !matches!(which, Sys::CancelMode) {
                assert_eq!(
                    st.dragging, before.dragging,
                    "{which:?} keeps the selection"
                );
            }
        }
        _ => {}
    }
}

fn toolbar(button: i32, before: &Snapshot, scene: &Scene) {
    let st = s();
    match button {
        CLOSE_BUTTON => {
            assert!(
                !st.active && !st.toolbar_open,
                "the × ends the snip and closes the toolbar"
            );
        }
        SETTINGS_BUTTON => assert!(
            !st.active && shown(scene.settings),
            "the gear opens settings"
        ),
        HISTORY_BUTTON => assert!(!st.active && shown(scene.history), "the history opens"),
        EDIT_BUTTON => assert_eq!(st.active, before.active, "the pen button only switches"),
        _ => {
            if !before.active {
                assert!(st.active, "a kept toolbar starts the next snip");
                assert_toolbar_placed(before);
            }
        }
    }
}

fn toast_click(spot: Spot, before: &Snapshot, scene: &Scene, opened: &[(String, Option<String>)]) {
    let look = before.toast.as_ref().expect("a notification");
    assert!(closing_or_gone(), "a click on the notification closes it");
    if spot == Spot::Close {
        assert!(opened.is_empty(), "the × opens nothing");
        return;
    }
    if look.edit || look.path.is_none() {
        // 「マークアップと共有」は履歴の編集画面を開く。履歴がオフなら何も開かない。
        if before.history {
            assert!(shown(scene.history), "the notification opens the editor");
        }
    } else if let Some(path) = &look.path {
        let target = path.to_string_lossy();
        assert!(
            opened.iter().any(|(opened, _)| *opened == target),
            "the notification opens the saved file"
        );
    }
}
