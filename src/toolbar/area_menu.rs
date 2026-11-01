//! 範囲のドロップダウン。Snipping Tool と同じく、項目ごとにアイコンと名前を並べ、いま選んでいる
//! 項目の左にアクセントの縦線を描く。標準のポップアップメニューの項目を自前で描くので、
//! キーボードの操作や閉じ方は標準のメニューのまま。

use super::*;
use windows_sys::Win32::{
    Graphics::Gdi::BitBlt,
    UI::Controls::{DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_SELECTED},
};

const ITEM_WIDTH: f32 = 176.0;
const ITEM_HEIGHT: f32 = 40.0;

/// ドロップダウンに並べる範囲（上から順）。
const MODES: [i32; 4] = [MODE_RECT, MODE_WINDOW, MODE_SCREEN, MODE_FREE];

/// ドロップダウンの上から `position` 番目（0 から）の範囲。
#[cfg(test)]
pub(crate) fn area_mode(position: usize) -> i32 {
    MODES[position]
}

/// 範囲の名前。
fn label(mode: i32) -> &'static str {
    match mode {
        MODE_FREE => tr("Freeform"),
        MODE_WINDOW => tr("Window"),
        MODE_SCREEN => tr("Full screen"),
        _ => tr("Rectangle"),
    }
}

/// 範囲のアイコン。ツールバーのボタンにも同じものを描く。
pub(super) fn glyph(mode: i32) -> &'static str {
    match mode {
        MODE_FREE => "\u{EA49}",
        MODE_WINDOW => "\u{E7C4}",
        MODE_SCREEN => "\u{E7F4}",
        _ => "\u{EF20}",
    }
}

/// WM_MEASUREITEM: 項目の大きさ。
pub(super) unsafe fn measure(lp: LPARAM) {
    let item = lp as *mut MEASUREITEMSTRUCT;
    if item.is_null() {
        return;
    }
    let scale = state().scale;
    (*item).itemWidth = (ITEM_WIDTH * scale).round() as u32;
    (*item).itemHeight = (ITEM_HEIGHT * scale).round() as u32;
}

/// WM_DRAWITEM: 項目 1 つを描く。`itemData` は範囲の番号。
pub(super) unsafe fn draw(lp: LPARAM) {
    let item = lp as *const DRAWITEMSTRUCT;
    if item.is_null() {
        return;
    }
    let item = &*item;
    let (width, height) = (
        item.rcItem.right - item.rcItem.left,
        item.rcItem.bottom - item.rcItem.top,
    );
    let Some(mut canvas) = ui::Canvas::new(width, height) else {
        return;
    };
    let s = state();
    let (theme, scale) = (&s.theme, s.scale);
    let mode = item.itemData as i32;
    canvas
        .pixels()
        .fill(if theme.dark { 0xff2b2b2b } else { 0xfffcfcfc });
    let (w, h) = (width as f32, height as f32);
    if item.itemState & ODS_SELECTED != 0 {
        canvas.fill_round_rect(
            [4.0 * scale, 2.0 * scale, w - 4.0 * scale, h - 2.0 * scale],
            4.0 * scale,
            theme.hover,
        );
    }
    if mode == s.mode {
        canvas.fill_round_rect(
            [
                4.0 * scale,
                h / 2.0 - 8.0 * scale,
                7.0 * scale,
                h / 2.0 + 8.0 * scale,
            ],
            1.5 * scale,
            theme.accent,
        );
    }
    let icon = ui::Font::new(ui::ICON_FONT, 16.0 * scale, FW_NORMAL as i32);
    let text = ui::Font::new(ui::TEXT_FONT, 14.0 * scale, FW_NORMAL as i32);
    canvas.text(
        glyph(mode),
        &icon,
        ui::rect(12.0 * scale, 0.0, 40.0 * scale, h),
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        theme.text,
    );
    canvas.text(
        label(mode),
        &text,
        ui::rect(48.0 * scale, 0.0, w - 8.0 * scale, h),
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        theme.text,
    );
    BitBlt(
        item.hDC,
        item.rcItem.left,
        item.rcItem.top,
        width,
        height,
        canvas.dc(),
        0,
        0,
        SRCCOPY,
    );
}

/// 範囲のボタンの下にドロップダウンを出し、選んだ範囲に切り替える。
pub(super) unsafe fn choose(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    for mode in MODES {
        // 項目の名前は WM_DRAWITEM で描くので、`lpNewItem` には範囲の番号を渡す。
        AppendMenuW(
            menu,
            MF_OWNERDRAW,
            mode as usize + 1,
            mode as usize as *const u16,
        );
    }
    let button = toolbar_button(AREA_BUTTON, state().scale);
    let mut anchor = POINT {
        x: button[0] as i32,
        y: button[3] as i32,
    };
    ClientToScreen(hwnd, &mut anchor);
    SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY,
        anchor.x,
        anchor.y,
        0,
        hwnd,
        null(),
    );
    DestroyMenu(menu);
    if command > 0 && state().active {
        set_mode(command - 1);
    } else if command > 0 {
        // A kept toolbar between snips starts a new snip in the chosen mode.
        state().mode = command - 1;
        begin_capture();
        if state().active {
            set_mode(command - 1);
        }
    }
    if state().active {
        SetForegroundWindow(state().overlay);
    }
}
