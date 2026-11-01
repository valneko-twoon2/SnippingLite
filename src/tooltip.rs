//! ボタンのヒント。Snipping Tool と同じく、アイコンのボタンにマウスをしばらく置くと、ボタンの下に
//! 名前を出す。ツールバーと書き込みのバーが使う。ヒントの窓は 1 つで、出すたびに文字と位置を変える。
//!
//! ボタンを持つ窓は、マウスの下のボタンが変わったら `hover`、マウスが離れたり押されたり、窓が
//! 隠れたりしたら `hide` を呼ぶ。

use super::*;

/// ヒントの窓のクラス名。
pub(crate) const CLASS: &str = "SnippingLiteTip";
/// マウスを置いてからヒントを出すまで（ミリ秒）。Windows のヒントの既定と同じ。
const DELAY: u32 = 500;
/// ヒントを出している間に、ほかのボタンへ移ったときの待ち（ミリ秒）。
const RESHOW: u32 = 100;
const SHOW_TIMER: usize = 1;
/// ヒントの高さと、文字の左右の余白（DIP）。
const HEIGHT: f32 = 28.0;
const PADDING: f32 = 10.0;
/// ボタンの下端からヒントまで（DIP）。バーの下端から 4 DIP 離れる。
const GAP: f32 = 12.0;

/// 出す（出している）ヒント。
pub(crate) struct Tip {
    window: HWND,
    /// ボタンを持つ窓。
    owner: HWND,
    /// 出す文字。空なら出さない。
    text: &'static str,
    /// ボタンの矩形（画面の座標）。
    anchor: RECT,
}

impl Tip {
    pub(crate) const fn new() -> Tip {
        Tip {
            window: null_mut(),
            owner: null_mut(),
            text: "",
            anchor: RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
        }
    }
}

/// ヒントの左上（画面の座標）。ボタン `anchor` の下に中央をそろえて置き、作業領域 `work` の下に
/// 入らなければボタンの上に置く。左右は作業領域の中に収める。
pub(crate) fn place(anchor: RECT, (width, height): (i32, i32), work: RECT, scale: f32) -> POINT {
    let gap = (GAP * scale).round() as i32;
    let x = ((anchor.left + anchor.right) / 2 - width / 2)
        .min(work.right - width)
        .max(work.left);
    let y = if anchor.bottom + gap + height <= work.bottom {
        anchor.bottom + gap
    } else {
        anchor.top - gap - height
    };
    POINT { x, y }
}

/// `owner` のマウスの下のボタンが変わった。`button` はそのボタンの矩形（クライアント座標）と
/// ヒントの文字で、ヒントのないボタンやボタンの外なら `None`。ヒントはしばらくしてから出す。
pub(crate) unsafe fn hover(owner: HWND, button: Option<([f32; 4], &'static str)>) {
    let quick = shown();
    hide();
    let Some(([left, top, right, bottom], text)) = button else {
        return;
    };
    let window = create();
    if window.is_null() {
        return;
    }
    let mut corners = [
        POINT {
            x: left.round() as i32,
            y: top.round() as i32,
        },
        POINT {
            x: right.round() as i32,
            y: bottom.round() as i32,
        },
    ];
    for corner in &mut corners {
        ClientToScreen(owner, corner);
    }
    let tip = &mut state().tip;
    tip.owner = owner;
    tip.text = text;
    tip.anchor = RECT {
        left: corners[0].x,
        top: corners[0].y,
        right: corners[1].x,
        bottom: corners[1].y,
    };
    SetTimer(window, SHOW_TIMER, if quick { RESHOW } else { DELAY }, None);
}

/// ヒントを隠し、出す予定も取りやめる。
pub(crate) unsafe fn hide() {
    let tip = &mut state().tip;
    tip.text = "";
    if !tip.window.is_null() {
        KillTimer(tip.window, SHOW_TIMER);
        ShowWindow(tip.window, SW_HIDE);
    }
}

/// ヒントが見えているか。
pub(crate) unsafe fn shown() -> bool {
    let window = state().tip.window;
    !window.is_null() && IsWindowVisible(window) != 0
}

/// 出している（出す予定の）文字。
#[cfg(test)]
pub(crate) unsafe fn text() -> &'static str {
    state().tip.text
}

/// ヒントの窓。初めて使うときに作る。撮影や録画に写らないようにする。ツールバーや書き込みの
/// バーと同じくオーバーレイの持ち物にし、オーバーレイとともに消える。
unsafe fn create() -> HWND {
    let s = state();
    let tip = &mut s.tip;
    if tip.window.is_null() {
        tip.window = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w(CLASS).as_ptr(),
            w(CLASS).as_ptr(),
            WS_POPUP,
            0,
            0,
            1,
            1,
            s.overlay,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if !tip.window.is_null() {
            SetWindowDisplayAffinity(tip.window, WDA_EXCLUDEFROMCAPTURE);
            ui::disable_transitions(tip.window);
        }
    }
    tip.window
}

/// 文字の幅（画素）。
unsafe fn text_width(text: &str, font: &ui::Font) -> i32 {
    let dc = CreateCompatibleDC(null_mut());
    if dc.is_null() {
        return 0;
    }
    let old = SelectObject(dc, font.0);
    let text: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE { cx: 0, cy: 0 };
    GetTextExtentPoint32W(dc, text.as_ptr(), text.len() as i32, &mut size);
    SelectObject(dc, old);
    DeleteDC(dc);
    size.cx
}

unsafe fn font(scale: f32) -> ui::Font {
    ui::Font::new(ui::TEXT_FONT, 12.0 * scale, FW_NORMAL as i32)
}

/// 待ちが明けた。ボタンを持つ窓がまだ見えていれば、ボタンのそばにヒントを出す。
unsafe fn show() {
    let s = state();
    let tip = &s.tip;
    if tip.text.is_empty() || IsWindowVisible(tip.owner) == 0 {
        return;
    }
    let scale = s.scale;
    let width = text_width(tip.text, &font(scale)) + (2.0 * PADDING * scale).round() as i32;
    let height = (HEIGHT * scale).round() as i32;
    let mut info: MONITORINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    GetMonitorInfoW(
        MonitorFromRect(&tip.anchor, MONITOR_DEFAULTTONEAREST),
        &mut info,
    );
    let at = place(tip.anchor, (width, height), info.rcWork, scale);
    ui::acrylic(tip.window, s.theme.dark);
    SetWindowPos(
        tip.window,
        HWND_TOPMOST,
        at.x,
        at.y,
        width,
        height,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
    InvalidateRect(tip.window, null(), 0);
    UpdateWindow(tip.window);
}

unsafe fn paint(hwnd: HWND, dc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);
    let Some(mut canvas) = ui::Canvas::new(client.right, client.bottom) else {
        return;
    };
    let s = state();
    canvas.surface(s.theme.dark, s.scale);
    canvas.text(
        s.tip.text,
        &font(s.scale),
        client,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        s.theme.text,
    );
    canvas.present(dc);
}

pub(crate) unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        // マウスは下の窓（同じスレッドのオーバーレイなど）へ通す。
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_ERASEBKGND => 1,
        WM_TIMER if wp == SHOW_TIMER => {
            KillTimer(hwnd, SHOW_TIMER);
            show();
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            paint(hwnd, dc);
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

#[cfg(test)]
mod tests;
