//! 操作を本物のウィンドウへ送る。届く所にだけ送る: 隠れたウィンドウにはマウスもキーも届かず、
//! オーバーレイが閉じた後の残りの動き（ダブルクリックの 2 回目など）は送らない。確定になり
//! うるメッセージは、送る前に仕様から期待を決め、送った後に届け先と比べる（sinks.rs）。

use super::actions::{Action, Spot, Start, Sys, P};
use super::sinks::{verify, Expect, Flags, Sinks};
use super::*;

fn lparam((x, y): P) -> LPARAM {
    ((y as u16 as isize) << 16) | (x as u16 as isize)
}

/// ボタンの真ん中（クライアント座標）。
fn middle([left, top, right, bottom]: [f32; 4]) -> P {
    (((left + right) / 2.0) as i32, ((top + bottom) / 2.0) as i32)
}

/// キューに来た本物の機器の入力か。テストの入力は SendMessage で直接届けるのでキューには来ない。
/// 本物のマウスの位置（マウスをつかんだ窓に、システムが作る WM_MOUSEMOVE など）は、テストが
/// 演じるマウスと食い違うので届けない。メニューに積んだキー（窓のないスレッドへのメッセージ）は通す。
fn device_input(message: &MSG) -> bool {
    // WM_NCMOUSEMOVE から WM_NCXBUTTONDBLCLK まで、WM_NCMOUSEHOVER から WM_MOUSELEAVE まで。
    const NONCLIENT_MOUSE: u32 = 0x00A0;
    const NONCLIENT_MOUSE_LAST: u32 = 0x00AD;
    const HOVER_AND_LEAVE: u32 = 0x02A0;
    const HOVER_AND_LEAVE_LAST: u32 = 0x02A3;
    !message.hwnd.is_null()
        && matches!(
            message.message,
            WM_MOUSEFIRST..=WM_MOUSELAST
                | WM_KEYFIRST..=WM_KEYLAST
                | NONCLIENT_MOUSE..=NONCLIENT_MOUSE_LAST
                | HOVER_AND_LEAVE..=HOVER_AND_LEAVE_LAST
        )
}

/// たまったメッセージ（描画、タイマー、メニューのコマンド）を `duration` の間流す。
/// `Desk::pump` と同じだが、本物の機器の入力は捨てる。
pub(super) fn pump(duration: Duration) {
    let end = Instant::now() + duration;
    loop {
        // SAFETY: plain message loop calls on this thread's queue.
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                if device_input(&message) {
                    continue;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if Instant::now() >= end {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// 範囲 `a`〜`b` を画面（`width` × `height`）に収めた幅と高さ。
fn span(a: POINT, b: POINT, width: i32, height: i32) -> (i32, i32) {
    let w = a.x.max(b.x).clamp(0, width) - a.x.min(b.x).clamp(0, width);
    let h = a.y.max(b.y).clamp(0, height) - a.y.min(b.y).clamp(0, height);
    (w, h)
}

pub(super) struct Hands<'a> {
    pub desk: &'a Desk,
    pub world: &'a mut World,
    pub scene: &'a Scene,
}

impl Hands<'_> {
    /// 送って、出てきたカウントダウンのタイマーを止める（本物の録画を始めないため）。
    fn send(&mut self, hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        let result = self.desk.send(hwnd, message, wp, lp);
        self.after_send();
        result
    }

    fn after_send(&mut self) {
        let st = s();
        if !st.count_window.is_null() && st.count_window != self.world.countdown {
            // SAFETY: the countdown window belongs to this thread.
            unsafe { KillTimer(st.count_window, COUNT_TIMER) };
            self.world.countdown = st.count_window;
        }
        // SAFETY: plain query on this thread.
        let captured = unsafe { GetCapture() } == st.overlay;
        if !shown(st.overlay) && !captured {
            // 押したままのボタンを離しても、もうオーバーレイには届かない。
            self.world.held = false;
        }
    }

    /// 送ったメッセージが `expect` のとおりに画像を届けたか（届けなかったか）を確かめる。
    fn confirming(&mut self, expect: Expect, what: &str, send: impl FnOnce(&mut Self)) {
        let flags = Flags::now();
        let before = self.world.last;
        send(self);
        let label = format!("{what} ({expect:?})");
        self.world.delivered += verify(expect, flags, &before, &self.world.shots, &label);
        self.world.last = Sinks::observe(&self.world.shots);
    }

    fn overlay_reachable(&self) -> bool {
        let overlay = s().overlay;
        // SAFETY: plain query on this thread.
        shown(overlay) || unsafe { GetCapture() } == overlay
    }

    fn overlay_move(&mut self, p: P) {
        if self.overlay_reachable() {
            self.world.last_point = p;
            self.send(s().overlay, WM_MOUSEMOVE, 0, lparam(p));
        }
    }

    fn overlay_down(&mut self, p: P) {
        if !self.overlay_reachable() {
            return;
        }
        let expect = self.expect_down();
        self.confirming(expect, "pressing on the overlay", |hands| {
            hands.world.held = true;
            hands.send(s().overlay, WM_LBUTTONDOWN, 1, lparam(p));
        });
    }

    fn overlay_up(&mut self, p: P) {
        if !self.overlay_reachable() {
            self.world.held = false;
            return;
        }
        let expect = self.expect_up(p);
        self.confirming(expect, "releasing on the overlay", |hands| {
            hands.world.held = false;
            hands.send(s().overlay, WM_LBUTTONUP, 0, lparam(p));
        });
    }

    /// 押したときに確定するもの: 全画面はどこを押しても、ウィンドウは選んだ窓があるとき。
    fn expect_down(&self) -> Expect {
        let st = s();
        // SAFETY: the state belongs to this thread.
        if !st.active || unsafe { markup::probe::look() }.is_some() {
            return Expect::Nothing;
        }
        match st.mode {
            MODE_SCREEN => Expect::Selection(Some((st.width, st.height))),
            MODE_WINDOW if st.hover_window.is_some() => {
                Expect::Selection(self.scene.targets.size_of(st.hover_hwnd))
            }
            _ => Expect::Nothing,
        }
    }

    /// 離したときに確定するもの: 選んでいる範囲が空でないとき。自由形式は 3 点以上を囲む矩形。
    fn expect_up(&self, p: P) -> Expect {
        let st = s();
        // SAFETY: the state belongs to this thread.
        if !st.active || !st.dragging || unsafe { markup::probe::look() }.is_some() {
            return Expect::Nothing;
        }
        let size = if st.mode == MODE_FREE {
            if st.points.len() < 3 {
                return Expect::Nothing;
            }
            let low = POINT {
                x: st.points.iter().map(|q| q.x).min().unwrap_or(0),
                y: st.points.iter().map(|q| q.y).min().unwrap_or(0),
            };
            let high = POINT {
                x: st.points.iter().map(|q| q.x).max().unwrap_or(0),
                y: st.points.iter().map(|q| q.y).max().unwrap_or(0),
            };
            span(low, high, st.width, st.height)
        } else {
            span(st.start, POINT { x: p.0, y: p.1 }, st.width, st.height)
        };
        if size.0 < 1 || size.1 < 1 {
            return Expect::Nothing;
        }
        Expect::Selection(Some(size))
    }

    fn key(&mut self, key: u16, control: bool) {
        if !shown(s().overlay) {
            return;
        }
        // SAFETY: the state belongs to this thread.
        let session = unsafe { markup::probe::look() };
        let confirms = key == VK_RETURN || (control && key == b'C' as u16);
        let expect = match session {
            Some(look) if confirms => Expect::Markup((look.width, look.height)),
            _ => Expect::Nothing,
        };
        self.confirming(expect, "a key on the overlay", |hands| {
            scene::with_control(control, || {
                hands.send(s().overlay, WM_KEYDOWN, key as usize, 0);
                if shown(s().overlay) {
                    hands.send(s().overlay, WM_KEYUP, key as usize, 0);
                }
            });
        });
    }

    fn toolbar_point(&self, button: i32) -> P {
        middle(toolbar_button(button, s().scale))
    }

    /// ツールバーのボタンをクリックする。`expect` はそのクリックで確定するもの。
    fn toolbar_click(&mut self, button: i32, expect: Expect) {
        let toolbar = s().toolbar;
        if !shown(toolbar) {
            return;
        }
        let p = self.toolbar_point(button);
        self.send(toolbar, WM_MOUSEMOVE, 0, lparam(p));
        self.send(toolbar, WM_LBUTTONDOWN, 1, lparam(p));
        self.confirming(expect, "a toolbar button", |hands| {
            hands.send(toolbar, WM_LBUTTONUP, 0, lparam(p));
        });
        match button {
            CAMERA_BUTTON | VIDEO_BUTTON => self.world.video = button == VIDEO_BUTTON,
            EDIT_BUTTON => self.world.markup = !self.world.markup,
            _ => {}
        }
    }

    fn area_menu(&mut self, choice: Option<usize>) {
        if !shown(s().toolbar) {
            return;
        }
        let expect = match choice {
            Some(mode) if mode as i32 == MODE_SCREEN => {
                // SAFETY: plain metric queries.
                let size = unsafe {
                    (
                        GetSystemMetrics(SM_CXVIRTUALSCREEN),
                        GetSystemMetrics(SM_CYVIRTUALSCREEN),
                    )
                };
                Expect::Selection(Some(size))
            }
            _ => Expect::Nothing,
        };
        scene::queue_menu(choice);
        self.toolbar_click(AREA_BUTTON, expect);
        if let Some(mode) = choice {
            self.world.mode = mode as i32;
        }
    }

    fn bar_click(&mut self, button: i32) {
        let bar = s().markup_bar;
        if !shown(bar) {
            return;
        }
        let p = middle(markup::probe::button(button, s().scale));
        self.send(bar, WM_MOUSEMOVE, 0, lparam(p));
        self.send(bar, WM_LBUTTONDOWN, 1, lparam(p));
        // SAFETY: the state belongs to this thread.
        let session = unsafe { markup::probe::look() };
        let expect = match session {
            Some(look) if button == markup::probe::CAPTURE => {
                Expect::Markup((look.width, look.height))
            }
            _ => Expect::Nothing,
        };
        self.confirming(expect, "a markup bar button", |hands| {
            hands.send(bar, WM_LBUTTONUP, 0, lparam(p));
        });
    }

    /// あるボタンで押し、別のボタンの上で離す。クリックではないので、何も起きない。
    fn slip(&mut self, hwnd: HWND, from: P, to: P) {
        if shown(hwnd) {
            self.send(hwnd, WM_MOUSEMOVE, 0, lparam(from));
            self.send(hwnd, WM_LBUTTONDOWN, 1, lparam(from));
            self.send(hwnd, WM_MOUSEMOVE, 1, lparam(to));
            self.send(hwnd, WM_LBUTTONUP, 0, lparam(to));
        }
    }

    /// 通知の上の場所（クライアント座標）。
    fn toast_point(spot: Spot, look: &toast::probe::Look) -> P {
        let at = |x: f32, y: f32| ((x * look.scale) as i32, (y * look.scale) as i32);
        match spot {
            Spot::Close => at(334.0, 24.0),
            Spot::Menu => at(300.0, 24.0),
            Spot::Button if look.body => at(200.0, 124.0),
            Spot::Button => at(200.0, 104.0),
            Spot::Text => at(20.0, 80.0),
        }
    }

    fn toast_click(&mut self, spot: Spot, menu: Option<Option<usize>>) {
        // SAFETY: the state belongs to this thread.
        let Some(look) = (unsafe { toast::probe::look() }) else {
            return;
        };
        let p = Self::toast_point(spot, &look);
        self.send(look.hwnd, WM_MOUSEMOVE, 0, lparam(p));
        self.send(look.hwnd, WM_LBUTTONDOWN, 1, lparam(p));
        // SAFETY: as above.
        let hover = unsafe { toast::probe::look() }.is_some_and(|t| t.menu_hover);
        assert_eq!(
            hover,
            spot == Spot::Menu,
            "only the menu button opens the menu"
        );
        if hover {
            scene::queue_menu(menu.flatten());
        }
        self.send(look.hwnd, WM_LBUTTONUP, 0, lparam(p));
    }

    fn system(&mut self, which: Sys) {
        let st = s();
        match which {
            Sys::DisplayChange => {
                for window in [self.desk.main, st.overlay, st.toolbar] {
                    self.send(window, WM_DISPLAYCHANGE, 32, lparam((1920, 1080)));
                }
            }
            Sys::DpiChanged => {
                for window in [st.overlay, st.toolbar] {
                    // 提案する大きさは今のまま。DefWindowProc が動かしても何も変わらない。
                    let mut rect: RECT = RECT {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    };
                    // SAFETY: plain query; the rectangle outlives the synchronous send.
                    unsafe { GetWindowRect(window, &mut rect) };
                    let dpi = (96 << 16) | 96;
                    self.send(window, WM_DPICHANGED, dpi, &rect as *const RECT as LPARAM);
                }
            }
            Sys::SettingChange => {
                let area = w("ImmersiveColorSet");
                self.send(self.desk.main, WM_SETTINGCHANGE, 0, area.as_ptr() as LPARAM);
            }
            Sys::CancelMode => {
                // 別のウィンドウが前に出るときなどに届き、マウスをつかんでいれば放させる。
                if shown(st.overlay) {
                    self.send(st.overlay, WM_CANCELMODE, 0, 0);
                }
            }
        }
    }
}

mod apply;
pub(super) use apply::{apply, settle};
