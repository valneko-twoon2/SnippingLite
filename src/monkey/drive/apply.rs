//! 操作の種類ごとに、送るメッセージの並び。

use super::*;

/// 操作を送り、送り終えたらたまったメッセージ（描画、メニューのコマンド、マウスが離れた知らせ）を流す。
pub(in crate::monkey) fn apply(hands: &mut Hands, action: &Action) {
    let st = s();
    let main = hands.desk.main;
    match action {
        Action::Start(start) => {
            let (message, wp, lp) = match start {
                Start::Icon => (TRAY_MESSAGE, 1, WM_LBUTTONUP as LPARAM),
                Start::Shortcut => (WM_HOTKEY, HOTKEY as usize, 0),
                Start::New => (WM_COMMAND, NEW, 0),
            };
            hands.send(main, message, wp, lp);
        }
        Action::TrayMenu(choice) => {
            scene::queue_menu(*choice);
            hands.send(main, TRAY_MESSAGE, 1, WM_RBUTTONUP as LPARAM);
        }
        &Action::Command(message, command) => {
            hands.send(main, message, command, 0);
        }
        Action::Reload(settings) => {
            config::save_settings(settings).expect("the settings are written");
            hands.send(main, TRAY_RELOAD, 0, 0);
        }
        &Action::Move(p) => hands.overlay_move(p),
        Action::Hold(points) | Action::Drag(points) => {
            hands.overlay_move(points[0]);
            hands.overlay_down(points[0]);
            for &p in &points[1..] {
                hands.overlay_move(p);
            }
            if matches!(action, Action::Drag(_)) {
                hands.overlay_up(points[points.len() - 1]);
            }
        }
        &Action::Release(p) => {
            hands.overlay_move(p);
            hands.overlay_up(p);
        }
        &Action::Click(p) => {
            hands.overlay_move(p);
            hands.overlay_down(p);
            hands.overlay_up(p);
        }
        &Action::DoubleClick(p) => {
            hands.overlay_move(p);
            for _ in 0..2 {
                hands.overlay_down(p);
                hands.overlay_up(p);
            }
        }
        &Action::RightClick(p) => {
            hands.overlay_move(p);
            for message in [WM_RBUTTONDOWN, WM_RBUTTONUP] {
                if shown(st.overlay) {
                    hands.send(st.overlay, message, 0, lparam(p));
                }
            }
        }
        &Action::Wheel(p, delta) => {
            if shown(st.overlay) {
                hands.send(
                    st.overlay,
                    WM_MOUSEWHEEL,
                    (delta as u16 as usize) << 16,
                    lparam(p),
                );
            }
        }
        &Action::Key(key, control) => hands.key(key, control),
        &Action::Toolbar(button) => hands.toolbar_click(button, Expect::Nothing),
        &Action::ToolbarSlip(from, to) => {
            let (a, b) = (hands.toolbar_point(from), hands.toolbar_point(to));
            hands.slip(st.toolbar, a, b);
        }
        &Action::ToolbarHover(button) => {
            if shown(st.toolbar) {
                let p = hands.toolbar_point(button);
                hands.send(st.toolbar, WM_MOUSEMOVE, 0, lparam(p));
            }
        }
        Action::ToolbarLeave => {
            if shown(st.toolbar) {
                hands.send(st.toolbar, WM_MOUSELEAVE, 0, 0);
            }
        }
        &Action::AreaMenu(choice) => hands.area_menu(choice),
        &Action::MoveToolbar((x, y)) => {
            if shown(st.toolbar) {
                let toolbar = st.toolbar;
                let mut rect: RECT = RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                // SAFETY: plain window calls on this thread's toolbar.
                unsafe { GetWindowRect(toolbar, &mut rect) };
                let gap = (
                    rect.left + (86.0 * s().scale) as i32,
                    rect.top + (24.0 * s().scale) as i32,
                );
                let hit = hands.send(toolbar, WM_NCHITTEST, 0, lparam(gap));
                assert_eq!(
                    hit, HTCAPTION as LRESULT,
                    "the space between the buttons moves the toolbar"
                );
                hands.send(toolbar, WM_ENTERSIZEMOVE, 0, 0);
                // SAFETY: as above.
                unsafe {
                    SetWindowPos(
                        toolbar,
                        null_mut(),
                        x,
                        y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                };
                hands.send(toolbar, WM_EXITSIZEMOVE, 0, 0);
                hands.send(toolbar, WM_NCLBUTTONDBLCLK, HTCAPTION as usize, lparam(gap));
            }
        }
        &Action::Bar(button) => hands.bar_click(button),
        &Action::BarSlip(from, to) => {
            let point = |index| middle(markup::probe::button(index, s().scale));
            hands.slip(st.markup_bar, point(from), point(to));
        }
        &Action::BarHover(button) => {
            if shown(st.markup_bar) {
                let p = middle(markup::probe::button(button, s().scale));
                hands.send(st.markup_bar, WM_MOUSEMOVE, 0, lparam(p));
            }
        }
        Action::BarLeave => {
            if shown(st.markup_bar) {
                hands.send(st.markup_bar, WM_MOUSELEAVE, 0, 0);
            }
        }
        &Action::ToastHover(spot) => {
            // SAFETY: the state belongs to this thread.
            if let Some(look) = unsafe { toast::probe::look() } {
                let p = Hands::toast_point(spot, &look);
                hands.send(look.hwnd, WM_MOUSEMOVE, 0, lparam(p));
            }
        }
        &Action::ToastClick(spot) => hands.toast_click(spot, None),
        Action::ToastLeave => {
            // SAFETY: as above.
            if let Some(look) = unsafe { toast::probe::look() } {
                hands.send(look.hwnd, WM_MOUSELEAVE, 0, 0);
            }
        }
        &Action::ToastMenu(choice) => hands.toast_click(Spot::Menu, Some(choice)),
        &Action::ToastTimer(timer) => {
            // SAFETY: as above.
            if let Some(look) = unsafe { toast::probe::look() } {
                hands.send(look.hwnd, WM_TIMER, timer, 0);
            }
        }
        Action::Tick => {
            if !st.count_window.is_null() && st.count_left >= 2 {
                hands.send(st.count_window, WM_TIMER, COUNT_TIMER, 0);
            }
        }
        Action::CountPaint => {
            if !st.count_window.is_null() {
                // SAFETY: the countdown window belongs to this thread.
                unsafe {
                    InvalidateRect(st.count_window, null(), 0);
                    UpdateWindow(st.count_window);
                }
            }
        }
        Action::CountClick => {
            if !st.count_window.is_null() {
                let window = st.count_window;
                hands.send(window, WM_MOUSEMOVE, 0, lparam((10, 10)));
                hands.send(window, WM_LBUTTONDOWN, 1, lparam((10, 10)));
                hands.send(window, WM_LBUTTONUP, 0, lparam((10, 10)));
            }
        }
        &Action::System(which) => hands.system(which),
        &Action::Wait(ms) => pump(Duration::from_millis(ms)),
    }
    hands.after_send();
    pump(Duration::ZERO);
    hands.after_send();
}

/// ケースの終わりに待機へ戻す: 押したままなら離し、撮影中なら Escape、カウントダウンは
/// クリックで取り消し、通知は閉じる。
pub(in crate::monkey) fn settle(hands: &mut Hands) {
    if hands.world.held {
        let p = hands.world.last_point;
        hands.overlay_up(p);
    }
    if shown(s().overlay) {
        hands.key(VK_ESCAPE, false);
    }
    let window = s().count_window;
    if !window.is_null() {
        hands.send(window, WM_LBUTTONUP, 0, lparam((10, 10)));
    }
    // SAFETY: the notification belongs to this thread.
    unsafe { toast::close() };
    pump(Duration::ZERO);
    hands.after_send();
    hands.world.last = Sinks::observe(&hands.world.shots);
}
