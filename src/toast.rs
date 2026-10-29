//! Notification shown after a capture, styled like a Windows 11 toast.

use std::{
    path::PathBuf,
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{
        Controls::WM_MOUSELEAVE,
        Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT},
        Shell::ShellExecuteW,
        WindowsAndMessaging::*,
    },
};

use crate::i18n::tr;
use crate::{
    point, state,
    ui::{self, Canvas, Font},
    w,
};

pub const CLASS: &str = "SnippingLiteToast";
const ANIMATION: usize = 1;
const DISMISS: usize = 2;
const WIDTH: f32 = 364.0;
const PADDING: f32 = 16.0;
const HEADER: f32 = 20.0;
const SLIDE_MS: f32 = 250.0;
const VISIBLE_MS: u32 = 6000;

pub struct Toast {
    hwnd: HWND,
    title: &'static str,
    body: String,
    path: Option<PathBuf>,
    can_open: bool,
    edit: bool,
    action_hover: bool,
    menu_hover: bool,
    scale: f32,
    left: i32,
    right: i32,
    top: i32,
    started: Instant,
    closing: bool,
    hover: bool,
    close_hover: bool,
}

/// Shows a notification for a finished capture, replacing an existing one.
pub unsafe fn show(pixels: &[u8], width: i32, height: i32, copied: bool, path: Option<PathBuf>) {
    let (title, body) = match (copied, &path) {
        (true, Some(_)) => (
            tr("Snip copied to clipboard"),
            tr("Saved automatically to the Screenshots folder").to_string(),
        ),
        (true, None) => (tr("Snip copied to clipboard"), String::new()),
        (false, Some(p)) => (tr("Snip saved"), display_name(p)),
        (false, None) => return,
    };
    present(pixels, width, height, title, body, path);
}

/// Shows a notification for a finished recording; `pixels` is its first frame.
pub unsafe fn show_recording(pixels: &[u8], width: i32, height: i32, path: PathBuf) {
    let body = display_name(&path);
    present(
        pixels,
        width,
        height,
        tr("Recording saved"),
        body,
        Some(path),
    );
}

unsafe fn present(
    _pixels: &[u8],
    _width: i32,
    _height: i32,
    title: &'static str,
    body: String,
    path: Option<PathBuf>,
) {
    close();
    let mut work: RECT = std::mem::zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, (&mut work as *mut RECT).cast(), 0);
    let scale = ui::scale_at(POINT {
        x: work.right - 1,
        y: work.bottom - 1,
    });
    let toast_height = if body.is_empty() { 136.0 } else { 156.0 };
    let toast_width = (WIDTH * scale).round() as i32;
    let toast_height = (toast_height * scale).round() as i32;
    let margin = (12.0 * scale).round() as i32;
    let right = work.right - margin;
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w(CLASS).as_ptr(),
        w("Snipping Lite Notification").as_ptr(),
        WS_POPUP,
        work.right,
        work.bottom - margin - toast_height,
        toast_width,
        toast_height,
        null_mut(),
        null_mut(),
        null_mut(),
        null(),
    );
    if hwnd.is_null() {
        return;
    }
    state().toast = Some(Box::new(Toast {
        hwnd,
        title,
        body,
        can_open: path.is_some() || state().history,
        edit: title != tr("Recording saved") && state().history,
        action_hover: false,
        menu_hover: false,
        path,
        scale,
        left: right - toast_width,
        right: work.right,
        top: work.bottom - margin - toast_height,
        started: Instant::now(),
        closing: false,
        hover: false,
        close_hover: false,
    }));
    ui::acrylic(hwnd, ui::theme().dark);
    ui::disable_transitions(hwnd);
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    SetTimer(hwnd, ANIMATION, 10, None);
    SetTimer(hwnd, DISMISS, VISIBLE_MS, None);
}

fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Removes the notification immediately.
pub unsafe fn close() {
    if let Some(toast) = state().toast.take() {
        DestroyWindow(toast.hwnd);
    }
}

pub unsafe fn visible() -> bool {
    state().toast.is_some()
}

unsafe fn begin_closing(toast: &mut Toast) {
    if !toast.closing {
        toast.closing = true;
        toast.started = Instant::now();
        KillTimer(toast.hwnd, DISMISS);
        SetTimer(toast.hwnd, ANIMATION, 10, None);
    }
}

fn close_button(scale: f32, width: i32) -> RECT {
    let size = 32.0 * scale;
    let right = width as f32 - 4.0 * scale;
    ui::rect(right - size, 8.0 * scale, right, 8.0 * scale + size)
}

unsafe fn paint(toast: &Toast, dc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(toast.hwnd, &mut client);
    let Some(mut canvas) = Canvas::new(client.right, client.bottom) else {
        return;
    };
    let theme = ui::theme();
    let s = toast.scale;
    let title_top = 48.0;
    canvas.surface(theme.dark, s);
    let small = Font::new(ui::TEXT_FONT, 12.0 * s, FW_NORMAL as i32);
    canvas.app_icon(
        (PADDING * s).round() as i32,
        (18.0 * s).round() as i32,
        (16.0 * s).round() as u32,
    );
    canvas.text(
        "Snipping Lite",
        &small,
        ui::rect(
            (PADDING + 24.0) * s,
            PADDING * s,
            (WIDTH - PADDING - 40.0) * s,
            (PADDING + HEADER) * s,
        ),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        theme.secondary,
    );
    {
        let button = close_button(s, client.right);
        if toast.close_hover {
            canvas.fill_round_rect(
                [
                    button.left as f32,
                    button.top as f32,
                    button.right as f32,
                    button.bottom as f32,
                ],
                4.0 * s,
                theme.hover,
            );
        }
        let glyph = Font::new(ui::ICON_FONT, 10.0 * s, FW_NORMAL as i32);
        canvas.text(
            "\u{E711}",
            &glyph,
            button,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            theme.text,
        );
    }
    let title = Font::new(ui::TEXT_FONT, 14.0 * s, FW_NORMAL as i32);
    let body = Font::new(ui::TEXT_FONT, 14.0 * s, FW_NORMAL as i32);
    canvas.text(
        toast.title,
        &title,
        ui::rect(
            PADDING * s,
            title_top * s,
            (WIDTH - PADDING) * s,
            (title_top + 20.0) * s,
        ),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        theme.text,
    );
    canvas.text(
        &toast.body,
        &body,
        ui::rect(
            PADDING * s,
            (title_top + 22.0) * s,
            (WIDTH - PADDING) * s,
            (title_top + 40.0) * s,
        ),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_PATH_ELLIPSIS,
        theme.secondary,
    );
    let menu = ui::rect((WIDTH - 84.0) * s, 8.0 * s, (WIDTH - 44.0) * s, 40.0 * s);
    let glyph = Font::new(ui::ICON_FONT, 12.0 * s, FW_NORMAL as i32);
    canvas.text(
        "\u{E712}",
        &glyph,
        menu,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        theme.secondary,
    );
    if toast.can_open {
        let top = if toast.body.is_empty() { 88.0 } else { 108.0 };
        let button = [
            PADDING * s,
            top * s,
            (WIDTH - PADDING) * s,
            (top + 32.0) * s,
        ];
        let border = if theme.dark { 0xff404040 } else { 0xffd0d0d0 };
        let fill = if theme.dark {
            if toast.action_hover {
                0xff353535
            } else {
                0xff2d2d2d
            }
        } else if toast.action_hover {
            0xffeeeeee
        } else {
            0xfffafafa
        };
        canvas.fill_round_rect(button, 4.0 * s, border);
        canvas.fill_round_rect(
            [button[0] + s, button[1] + s, button[2] - s, button[3] - s],
            3.0 * s,
            fill,
        );
        canvas.text(
            if toast.edit {
                tr("Markup and share")
            } else {
                tr("Open")
            },
            &body,
            ui::rect(button[0], button[1], button[2], button[3]),
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            theme.text,
        );
    }
    canvas.present(dc);
}

pub unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(toast) = state().toast.as_deref_mut().filter(|t| t.hwnd == hwnd) else {
        return DefWindowProcW(hwnd, msg, wp, lp);
    };
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint(toast, dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER if wp == DISMISS => {
            begin_closing(toast);
            0
        }
        WM_TIMER if wp == ANIMATION => {
            let t = (toast.started.elapsed().as_secs_f32() * 1000.0 / SLIDE_MS).min(1.0);
            let (from, to, eased) = if toast.closing {
                (toast.left, toast.right, t * t * t)
            } else {
                (toast.right, toast.left, 1.0 - (1.0 - t).powi(3))
            };
            let x = from + ((to - from) as f32 * eased).round() as i32;
            SetWindowPos(
                hwnd,
                null_mut(),
                x,
                toast.top,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if t >= 1.0 {
                KillTimer(hwnd, ANIMATION);
                if toast.closing {
                    close();
                }
            }
            0
        }
        WM_MOUSEMOVE => {
            let p = point(lp);
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut client);
            let b = close_button(toast.scale, client.right);
            let close_hover = p.x >= b.left && p.x < b.right && p.y >= b.top && p.y < b.bottom;
            let action_top = if toast.body.is_empty() { 88.0 } else { 108.0 };
            let action_hover = p.x >= (PADDING * toast.scale) as i32
                && p.x < ((WIDTH - PADDING) * toast.scale) as i32
                && p.y >= (action_top * toast.scale) as i32
                && p.y < ((action_top + 32.0) * toast.scale) as i32;
            let menu_hover = p.x >= ((WIDTH - 84.0) * toast.scale) as i32
                && p.x < ((WIDTH - 44.0) * toast.scale) as i32
                && p.y >= (8.0 * toast.scale) as i32
                && p.y < (40.0 * toast.scale) as i32;
            if !toast.hover
                || close_hover != toast.close_hover
                || action_hover != toast.action_hover
                || menu_hover != toast.menu_hover
            {
                if !toast.hover {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut track);
                    KillTimer(hwnd, DISMISS);
                }
                toast.hover = true;
                toast.close_hover = close_hover;
                toast.action_hover = action_hover;
                toast.menu_hover = menu_hover;
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_MOUSELEAVE => {
            toast.hover = false;
            toast.close_hover = false;
            toast.action_hover = false;
            toast.menu_hover = false;
            if !toast.closing {
                SetTimer(hwnd, DISMISS, VISIBLE_MS / 2, None);
            }
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_LBUTTONUP => {
            if toast.menu_hover {
                let menu = CreatePopupMenu();
                if !menu.is_null() {
                    AppendMenuW(menu, MF_STRING, 1, w(tr("Settings")).as_ptr());
                    AppendMenuW(menu, MF_STRING, 2, w(tr("Dismiss notification")).as_ptr());
                    let mut p = point(lp);
                    ClientToScreen(hwnd, &mut p);
                    SetForegroundWindow(hwnd);
                    let command = TrackPopupMenu(
                        menu,
                        TPM_RETURNCMD | TPM_NONOTIFY,
                        p.x,
                        p.y,
                        0,
                        hwnd,
                        null(),
                    );
                    DestroyMenu(menu);
                    if command == 1 {
                        crate::launch_settings();
                    }
                    // A popup runs a nested message loop: the timer may already have
                    // destroyed the toast, so reacquire it before closing.
                    if command != 0 {
                        if let Some(current) =
                            state().toast.as_deref_mut().filter(|t| t.hwnd == hwnd)
                        {
                            begin_closing(current);
                        }
                    }
                }
                return 0;
            } else if !toast.close_hover {
                if toast.edit || toast.path.is_none() {
                    // Screenshot actions open the built-in editor; recordings open their file.
                    if state().history {
                        crate::launch_history(true);
                    }
                } else if let Some(path) = &toast.path {
                    let file: Vec<u16> =
                        std::os::windows::ffi::OsStrExt::encode_wide(path.as_os_str())
                            .chain(Some(0))
                            .collect();
                    ShellExecuteW(
                        null_mut(),
                        w("open").as_ptr(),
                        file.as_ptr(),
                        null(),
                        null(),
                        SW_SHOWNORMAL,
                    );
                }
            }
            begin_closing(toast);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::AppTheme,
        test_support::{answer_dialog, answer_menu, desk::Desk, stub_window},
    };
    use std::time::Duration;

    fn current() -> &'static mut Toast {
        // SAFETY: the test owns the state and runs on the thread that created it.
        unsafe {
            state()
                .toast
                .as_deref_mut()
                .expect("a notification is shown")
        }
    }

    fn client(hwnd: HWND) -> RECT {
        // SAFETY: plain window query.
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            rect
        }
    }

    fn left_edge(hwnd: HWND) -> i32 {
        // SAFETY: plain window query.
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            GetWindowRect(hwnd, &mut rect);
            rect.left
        }
    }

    #[test]
    fn display_names_are_the_last_path_component() {
        assert_eq!(
            display_name(std::path::Path::new("D:\\Shots\\a b.png")),
            "a b.png"
        );
        assert_eq!(display_name(std::path::Path::new("a.png")), "a.png");
        assert_eq!(display_name(std::path::Path::new("")), "");
    }

    #[test]
    fn the_close_button_stays_inside_the_toast_on_every_scale() {
        for step in 4..=16 {
            let scale = step as f32 / 4.0;
            let width = (WIDTH * scale).round() as i32;
            let button = close_button(scale, width);
            assert!(button.left > 0 && button.right < width && button.top >= 0);
            assert_eq!(button.bottom - button.top, (32.0 * scale).round() as i32);
        }
    }

    #[test]
    #[ignore = "Shows notification windows"]
    fn what_the_notification_says_follows_where_the_image_went() {
        let desk = Desk::start();
        let path = desk.sandbox.path().join("Snip 1.png");
        // Decision table over (copied, saved): the title, the body and the button.
        for (copied, saved, shown, title, body, can_open) in [
            (
                true,
                true,
                true,
                "Snip copied to clipboard",
                "Saved automatically to the Screenshots folder",
                true,
            ),
            (true, false, true, "Snip copied to clipboard", "", true),
            (false, true, true, "Snip saved", "Snip 1.png", true),
        ] {
            // SAFETY: the windows belong to this test.
            unsafe {
                show(&[0; 4], 1, 1, copied, saved.then(|| path.clone()));
                assert_eq!(visible(), shown);
            }
            let toast = current();
            assert_eq!(toast.title, tr(title), "{copied} {saved}");
            assert_eq!(toast.body, tr(body), "{copied} {saved}");
            assert_eq!(toast.can_open, can_open);
            assert!(toast.edit, "screenshots are edited in the history window");
        }
        // Nothing was copied or saved: there is nothing to announce.
        unsafe {
            close();
            show(&[0; 4], 1, 1, false, None);
            assert!(!visible());
        }
        drop(desk);
    }

    #[test]
    #[ignore = "Shows notification windows"]
    fn a_recording_notification_opens_the_file_and_a_new_one_replaces_the_old() {
        let desk = Desk::start();
        let path = desk.sandbox.path().join("Snipping Lite 1.mp4");
        // SAFETY: the windows belong to this test.
        unsafe {
            show_recording(&[0; 4], 1, 1, path.clone());
            let first = current().hwnd;
            assert_eq!(current().title, tr("Recording saved"));
            assert_eq!(current().body, "Snipping Lite 1.mp4");
            assert!(!current().edit && current().can_open);
            show_recording(&[0; 4], 1, 1, path);
            assert_ne!(current().hwnd, first);
            assert_eq!(IsWindow(first), 0, "the old notification is gone");
            close();
            assert!(!visible());
            // Closing twice is harmless.
            close();
        }
        drop(desk);
    }

    #[test]
    #[ignore = "Shows notification windows"]
    fn a_notification_without_history_cannot_edit_and_has_no_button_without_a_file() {
        let desk = Desk::start_with(|settings| settings.history = false);
        // SAFETY: the windows belong to this test.
        unsafe {
            show(&[0; 4], 1, 1, true, None);
            assert!(!current().edit && !current().can_open);
            let rect = client(current().hwnd);
            // Height with an empty body is the shorter variant.
            assert_eq!(rect.bottom, (136.0 * current().scale).round() as i32);
            show(&[0; 4], 1, 1, true, Some(desk.sandbox.path().join("a.png")));
            assert!(current().can_open && !current().edit);
            assert_eq!(
                client(current().hwnd).bottom,
                (156.0 * current().scale).round() as i32
            );
        }
        drop(desk);
    }

    #[test]
    #[ignore = "Shows notification windows"]
    fn the_notification_slides_in_waits_and_slides_out() {
        let desk = Desk::start();
        // SAFETY: the windows belong to this test.
        unsafe { show(&[0; 4], 1, 1, true, None) };
        let hwnd = current().hwnd;
        let (left, right) = (current().left, current().right);
        assert!(left < right);
        assert!(
            desk.pump_until(Duration::from_secs(3), || left_edge(hwnd) == left),
            "it slides to its place"
        );
        assert!(!current().closing);
        // The dismiss timer starts the slide out; the window is removed at the end of it.
        desk.send(hwnd, WM_TIMER, DISMISS, 0);
        assert!(current().closing);
        // A second request while closing changes nothing.
        desk.send(hwnd, WM_TIMER, DISMISS, 0);
        assert!(
            desk.pump_until(Duration::from_secs(3), || unsafe { !visible() }),
            "it is removed when it has slid out"
        );
        assert_eq!(unsafe { IsWindow(hwnd) }, 0);
        // Messages for a window that is not the current notification are left to Windows.
        assert_eq!(desk.send(hwnd, WM_TIMER, DISMISS, 0), 0);
    }

    #[test]
    #[ignore = "Shows notification windows"]
    fn pointing_at_the_notification_highlights_and_pauses_it() {
        let desk = Desk::start();
        // SAFETY: the windows belong to this test.
        unsafe { show(&[0; 4], 1, 1, true, Some(desk.sandbox.path().join("a.png"))) };
        let hwnd = current().hwnd;
        let s = current().scale;
        let close_at = (320.0 * s) as i32;
        let spot = |x: f32, y: f32| ((x * s) as i32, (y * s) as i32);
        // Equivalence classes of the pointer position, and the hover flags each one sets.
        let table = [
            (spot(334.0, 24.0), (true, false, false)), // the close button
            (spot(200.0, 120.0), (false, true, false)), // the action button
            (spot(300.0, 24.0), (false, false, true)), // the menu button
            (spot(20.0, 80.0), (false, false, false)), // the text
        ];
        let _ = close_at;
        for ((x, y), (close, action, menu)) in table {
            desk.mouse(hwnd, WM_MOUSEMOVE, x, y);
            let toast = current();
            assert!(toast.hover);
            assert_eq!(
                (toast.close_hover, toast.action_hover, toast.menu_hover),
                (close, action, menu),
                "{x},{y}"
            );
            // Painting works in every state.
            unsafe {
                InvalidateRect(hwnd, null(), 0);
                UpdateWindow(hwnd);
            }
        }
        // Moving inside the same spot changes nothing; leaving clears all of it.
        desk.mouse(hwnd, WM_MOUSEMOVE, 21, 81);
        assert!(current().hover);
        desk.send(hwnd, WM_MOUSELEAVE, 0, 0);
        let toast = current();
        assert!(!toast.hover && !toast.close_hover && !toast.action_hover && !toast.menu_hover);
        // Leaving a notification that is closing does not bring the dismiss timer back.
        toast.closing = true;
        desk.send(hwnd, WM_MOUSELEAVE, 0, 0);
        assert_eq!(desk.send(hwnd, WM_ERASEBKGND, 0, 0), 1);
        assert_eq!(
            desk.send(hwnd, WM_MOUSEACTIVATE, 0, 0),
            MA_NOACTIVATE as LRESULT
        );
    }

    #[test]
    #[ignore = "Shows notification windows"]
    fn painting_covers_every_theme_and_button_layout() {
        for theme in [AppTheme::Light, AppTheme::Dark] {
            let desk = Desk::start();
            crate::ui::set_theme_choice(theme);
            for (copied, path) in [(true, None), (true, Some("a.png")), (false, Some("b.png"))] {
                let path = path.map(|name| desk.sandbox.path().join(name));
                // SAFETY: the windows belong to this test.
                unsafe {
                    show(&[0; 4], 1, 1, copied, path);
                    let hwnd = current().hwnd;
                    for (action, close_hover) in [(false, false), (true, false), (false, true)] {
                        let toast = current();
                        toast.action_hover = action;
                        toast.close_hover = close_hover;
                        InvalidateRect(hwnd, null(), 0);
                        UpdateWindow(hwnd);
                    }
                }
            }
            crate::ui::set_theme_choice(AppTheme::System);
        }
    }

    #[test]
    #[ignore = "Shows notification windows"]
    fn the_menu_button_offers_settings_and_dismiss() {
        let desk = Desk::start();
        let settings = stub_window(crate::SETTINGS_WINDOW);
        // SAFETY: the windows belong to this test.
        unsafe {
            ShowWindow(settings, SW_HIDE);
            show(&[0; 4], 1, 1, true, None);
        }
        let hwnd = current().hwnd;
        let s = current().scale;
        let at = ((300.0 * s) as i32, (24.0 * s) as i32);
        desk.mouse(hwnd, WM_MOUSEMOVE, at.0, at.1);
        assert!(current().menu_hover);
        // Choosing Settings opens the settings window and closes the notification.
        let answer = answer_menu(0);
        desk.mouse(hwnd, WM_LBUTTONUP, at.0, at.1);
        assert!(answer.join().unwrap());
        assert!(
            unsafe { IsWindowVisible(settings) } != 0,
            "the settings window was brought forward"
        );
        assert!(current().closing);
        desk.pump(Duration::from_millis(50));
        unsafe { close() };
        // Choosing Dismiss only closes it.
        unsafe { show(&[0; 4], 1, 1, true, None) };
        let hwnd = current().hwnd;
        desk.mouse(hwnd, WM_MOUSEMOVE, at.0, at.1);
        unsafe { ShowWindow(settings, SW_HIDE) };
        let answer = answer_menu(1);
        desk.mouse(hwnd, WM_LBUTTONUP, at.0, at.1);
        assert!(answer.join().unwrap());
        assert!(unsafe { IsWindowVisible(settings) } == 0);
        assert!(current().closing);
        // Leaving the menu without a choice leaves the notification as it is.
        unsafe { close() };
        unsafe { show(&[0; 4], 1, 1, true, None) };
        let hwnd = current().hwnd;
        desk.mouse(hwnd, WM_MOUSEMOVE, at.0, at.1);
        let answer = answer_menu(usize::MAX);
        desk.mouse(hwnd, WM_LBUTTONUP, at.0, at.1);
        assert!(answer.join().unwrap());
        assert!(!current().closing);
        unsafe { DestroyWindow(settings) };
    }

    #[test]
    #[ignore = "Shows notification windows"]
    fn clicking_the_notification_opens_the_editor_or_the_file_and_the_x_just_closes() {
        let desk = Desk::start();
        let history = stub_window(crate::HISTORY_WINDOW);
        // SAFETY: the windows belong to this test.
        unsafe {
            ShowWindow(history, SW_HIDE);
            show(&[0; 4], 1, 1, true, Some(desk.sandbox.path().join("a.png")));
        }
        let hwnd = current().hwnd;
        let s = current().scale;
        let in_text = ((20.0 * s) as i32, (80.0 * s) as i32);
        // A click on the body of a screenshot notification opens the history editor.
        desk.mouse(hwnd, WM_MOUSEMOVE, in_text.0, in_text.1);
        desk.mouse(hwnd, WM_LBUTTONUP, in_text.0, in_text.1);
        assert!(unsafe { IsWindowVisible(history) } != 0);
        assert!(current().closing);
        unsafe { close() };
        // The close button dismisses without opening anything.
        unsafe {
            ShowWindow(history, SW_HIDE);
            show(&[0; 4], 1, 1, true, None);
        }
        let hwnd = current().hwnd;
        let x = ((334.0 * s) as i32, (24.0 * s) as i32);
        desk.mouse(hwnd, WM_MOUSEMOVE, x.0, x.1);
        desk.mouse(hwnd, WM_LBUTTONUP, x.0, x.1);
        assert!(unsafe { IsWindowVisible(history) } == 0);
        assert!(current().closing);
        unsafe { close() };
        // Without history there is no editor to open: the click only closes it.
        unsafe { state().history = false };
        unsafe { show(&[0; 4], 1, 1, true, None) };
        let hwnd = current().hwnd;
        desk.mouse(hwnd, WM_MOUSEMOVE, in_text.0, in_text.1);
        desk.mouse(hwnd, WM_LBUTTONUP, in_text.0, in_text.1);
        assert!(unsafe { IsWindowVisible(history) } == 0);
        assert!(current().closing);
        unsafe { close() };
        // A recording notification opens the file itself.
        let video = desk.sandbox.path().join("missing.mp4");
        unsafe { show_recording(&[0; 4], 1, 1, video) };
        let hwnd = current().hwnd;
        desk.mouse(hwnd, WM_MOUSEMOVE, in_text.0, in_text.1);
        let answer = answer_dialog(2);
        desk.mouse(hwnd, WM_LBUTTONUP, in_text.0, in_text.1);
        let _ = answer.join();
        assert!(current().closing);
        unsafe {
            close();
            DestroyWindow(history);
        }
    }
}
