use super::*;

/// 置いた後の背景の窓の重なり順を固定するか。背景の窓も最前面の窓なので、前面に出るたびに
/// （ほかのプロセスの窓が閉じて Windows が前面に戻したときや、テストが前面を取り戻したとき）
/// 最前面の中のいちばん上へ上がり、アプリの残したツールバーや録画の表示を覆ってしまう。覆われた
/// ツールバーは見えず、クリックも背景の窓に当たる。重なり順は `topmost` でだけ変える。
static KEEP_PLACE: AtomicBool = AtomicBool::new(false);

pub unsafe fn fill(dc: HDC, r: RECT, rgb: u32) {
    let brush = CreateSolidBrush(colorref(rgb));
    FillRect(dc, &r, brush);
    DeleteObject(brush);
}

pub unsafe extern "system" fn backdrop_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match msg {
        // 前面に出ても重なり順を変えず、アプリの最前面の窓（ツールバーや録画の表示）の下に残る。
        WM_WINDOWPOSCHANGING if KEEP_PLACE.load(Ordering::Relaxed) && lp != 0 => {
            (*(lp as *mut WINDOWPOS)).flags |= SWP_NOZORDER;
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut client: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut client);
            fill(dc, client, BACKGROUND);
            if PANEL_SHOWN.load(Ordering::Relaxed) {
                fill(dc, PANEL, 0xffffff);
                fill(dc, SQUARE, CYAN);
                let font = CreateFontW(
                    -64,
                    0,
                    0,
                    0,
                    FW_NORMAL as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    OUT_DEFAULT_PRECIS as u32,
                    CLIP_DEFAULT_PRECIS as u32,
                    ANTIALIASED_QUALITY as u32,
                    DEFAULT_PITCH as u32,
                    wide("Segoe UI").as_ptr(),
                );
                let old = SelectObject(dc, font);
                SetBkMode(dc, TRANSPARENT as i32);
                SetTextColor(dc, 0);
                let mut area = RECT {
                    left: PANEL.left + 20,
                    top: PANEL.top,
                    right: SQUARE.left - 10,
                    bottom: PANEL.bottom,
                };
                let mut text: Vec<u16> = TEXT.encode_utf16().collect();
                DrawTextW(
                    dc,
                    text.as_mut_ptr(),
                    text.len() as i32,
                    &mut area,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                );
                SelectObject(dc, old);
                DeleteObject(font);
            }
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

const BACKDROP_CLASS: &str = "SnippingLiteE2EBackdrop";

/// プロセスを一度だけ整える。座標をモニターごとの物理的な画素で扱い（DPI を意識しないプロセスには、
/// 倍率のあるモニターの座標が換算されて見える）、背景の窓のクラスを登録する。ポインターの位置を
/// 覚える前（`on_desktop`）と、背景の窓を作る前に呼ぶ。
pub fn setup() {
    let class = wide(BACKDROP_CLASS);
    // SAFETY: the class name outlives the call and the window procedure is a plain function.
    SETUP.call_once(|| unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(backdrop_proc),
            hInstance: GetModuleHandleW(null()),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        });
    });
}

/// 主モニターとその範囲（画素）。
pub fn primary_monitor() -> (HMONITOR, RECT) {
    // SAFETY: plain monitor queries into a sized structure.
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(monitor, &mut info);
        (monitor, info.rcMonitor)
    }
}

/// A window of a known color over the whole primary monitor, so captures have
/// known pixels and a stray click lands on it instead of another app.
pub struct Backdrop {
    pub hwnd: HWND,
    pub monitor: RECT,
    pub scale: f32,
}

impl Backdrop {
    pub fn new() -> Backdrop {
        setup();
        // SAFETY: the window belongs to this thread and is destroyed on drop.
        unsafe {
            let class = wide(BACKDROP_CLASS);
            let (monitor, r) = primary_monitor();
            let (mut dpi, mut unused) = (96u32, 96u32);
            GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused);
            PANEL_SHOWN.store(false, Ordering::Relaxed);
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            assert!(!hwnd.is_null(), "Create the backdrop");
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
            set_backdrop_window(hwnd);
            settle(500);
            // 最前面のいちばん上に置いた後は、重なり順を `topmost` でだけ変える。
            KEEP_PLACE.store(true, Ordering::Relaxed);
            Backdrop {
                hwnd,
                monitor: r,
                scale: dpi as f32 / 96.0,
            }
        }
    }

    pub fn width(&self) -> i32 {
        self.monitor.right - self.monitor.left
    }

    pub fn height(&self) -> i32 {
        self.monitor.bottom - self.monitor.top
    }

    /// A screen point `(x, y)` pixels from the monitor's top-left corner.
    pub fn at(&self, x: i32, y: i32) -> POINT {
        POINT {
            x: self.monitor.left + x,
            y: self.monitor.top + y,
        }
    }

    pub fn rect(&self, r: RECT) -> RECT {
        offset(r, self.monitor.left, self.monitor.top)
    }

    pub fn show_panel(&self, shown: bool) {
        PANEL_SHOWN.store(shown, Ordering::Relaxed);
        // SAFETY: the window belongs to this thread.
        unsafe {
            InvalidateRect(self.hwnd, null(), 1);
            UpdateWindow(self.hwnd);
        }
        settle(300);
    }

    pub fn topmost(&self, on: bool) {
        KEEP_PLACE.store(false, Ordering::Relaxed);
        // SAFETY: the window belongs to this thread, so the position changes before the call returns.
        unsafe {
            SetWindowPos(
                self.hwnd,
                if on { HWND_TOPMOST } else { HWND_NOTOPMOST },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE,
            );
        }
        KEEP_PLACE.store(true, Ordering::Relaxed);
        settle(200);
    }

    /// The toolbar where a newly opened snip places it: the top center of the monitor.
    pub fn toolbar(&self) -> RECT {
        let width = (332.0 * self.scale).round() as i32;
        let left = (self.monitor.left + self.monitor.right - width) / 2;
        let top = self.monitor.top + (12.0 * self.scale).round() as i32;
        RECT {
            left,
            top,
            right: left + width,
            bottom: top + (48.0 * self.scale).round() as i32,
        }
    }

    /// The recording indicator at the top center of the monitor.
    pub fn indicator(&self) -> RECT {
        let width = (168.0 * self.scale).round() as i32;
        let left = (self.monitor.left + self.monitor.right - width) / 2;
        let top = self.monitor.top + (12.0 * self.scale).round() as i32;
        RECT {
            left,
            top,
            right: left + width,
            bottom: top + (48.0 * self.scale).round() as i32,
        }
    }

    /// A point `x` DIPs from the left of a bar, in the middle of its height.
    pub fn button(&self, bar: RECT, x: f32) -> POINT {
        POINT {
            x: bar.left + (x * self.scale).round() as i32,
            y: bar.top + (24.0 * self.scale).round() as i32,
        }
    }

    /// A free part of the backdrop for selections, right of center.
    pub fn free(&self) -> POINT {
        self.at(self.width() / 2 + 120, self.height() / 2)
    }
}

impl Drop for Backdrop {
    fn drop(&mut self) {
        set_backdrop_window(null_mut());
        KEEP_PLACE.store(false, Ordering::Relaxed);
        // SAFETY: the window belongs to this thread.
        unsafe { DestroyWindow(self.hwnd) };
        settle(100);
    }
}
