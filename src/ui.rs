//! Drawing helpers for the Windows 11 style surfaces (toolbar and notification).
//!
//! The surfaces use a DWM acrylic backdrop, so everything is rendered into a
//! premultiplied 32-bit DIB and copied to the window in one `BitBlt`.

use std::ptr::{null, null_mut};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::{
        LibraryLoader::GetModuleHandleW,
        Registry::{
            RegGetValueW, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
        },
    },
    UI::{
        Controls::MARGINS,
        HiDpi::*,
        WindowsAndMessaging::{
            FindWindowExW, GetPropW, GetWindowThreadProcessId, LoadImageW, SetPropW, SetWindowPos,
            HICON, HWND_TOPMOST, IMAGE_ICON, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        },
    },
};

use crate::{config::AppTheme, w};
use std::sync::atomic::{AtomicU8, Ordering};

mod canvas;
pub use canvas::*;
/// モニターの並びと DPI から窓を置く所を決める関数。
pub mod monitors;

pub const ICON_FONT: &str = "Segoe Fluent Icons";
pub const TEXT_FONT: &str = "Yu Gothic UI";

/// `window` が `process` のプロセスの窓か。`process` がなければ、どのプロセスの窓でもよい。
unsafe fn in_process(window: HWND, process: Option<u32>) -> bool {
    process.is_none_or(|id| {
        let mut owner = 0;
        GetWindowThreadProcessId(window, &mut owner);
        owner == id
    })
}

/// Finds a document by role even when settings and history share a caption.
/// `process` があれば、そのプロセスの窓だけを探す。なければ、すべてのプロセスの窓から探す。
pub unsafe fn find_app_window(identity: &str, process: Option<u32>) -> HWND {
    let mut previous = null_mut();
    loop {
        let window = FindWindowExW(
            null_mut(),
            previous,
            null(),
            w(crate::SETTINGS_TITLE).as_ptr(),
        );
        if window.is_null()
            || (!GetPropW(window, w(identity).as_ptr()).is_null() && in_process(window, process))
        {
            return window;
        }
        previous = window;
    }
}

/// クラスが `class` の最上位の窓。`process` は `find_app_window` と同じ。
pub unsafe fn find_class_window(class: &str, process: Option<u32>) -> HWND {
    let mut previous = null_mut();
    loop {
        let window = FindWindowExW(null_mut(), previous, w(class).as_ptr(), null());
        if window.is_null() || in_process(window, process) {
            return window;
        }
        previous = window;
    }
}

/// Tags the document belonging to this window process on its first frame.
pub unsafe fn identify_app_window(identity: &str) -> HWND {
    let mut previous = null_mut();
    loop {
        let window = FindWindowExW(
            null_mut(),
            previous,
            null(),
            w(crate::SETTINGS_TITLE).as_ptr(),
        );
        if window.is_null() {
            return window;
        }
        if in_process(window, Some(std::process::id())) {
            SetPropW(window, w(identity).as_ptr(), 1usize as HANDLE);
            return window;
        }
        previous = window;
    }
}

/// Colors are 0xAARRGGBB with straight alpha.
pub struct Theme {
    pub dark: bool,
    pub text: u32,
    pub secondary: u32,
    pub hover: u32,
    pub pressed: u32,
    pub selected: u32,
    pub divider: u32,
    pub accent: u32,
    /// Solid colors as COLORREF values for the settings window frame.
    pub text_ref: u32,
    pub bg_ref: u32,
}

/// The theme chosen in the settings, as `AppTheme` discriminants.
static THEME_CHOICE: AtomicU8 = AtomicU8::new(AppTheme::System as u8);

pub fn set_theme_choice(choice: AppTheme) {
    THEME_CHOICE.store(choice as u8, Ordering::Relaxed);
}

fn system_dark() -> bool {
    let mut light = 1u32;
    let mut size = 4u32;
    // SAFETY: the output buffer and its size describe `light`.
    let found = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize").as_ptr(),
            w("AppsUseLightTheme").as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut light as *mut u32).cast(),
            &mut size,
        )
    } == 0;
    found && light == 0
}

pub fn theme() -> Theme {
    let dark = match THEME_CHOICE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => system_dark(),
    };
    if dark {
        Theme {
            dark: true,
            text: 0xffffffff,
            secondary: 0xc8ffffff,
            hover: 0x0fffffff,
            pressed: 0x0affffff,
            selected: 0x15ffffff,
            divider: 0x15ffffff,
            accent: 0xff4cc2ff,
            text_ref: 0x00ffffff,
            bg_ref: 0x00202020,
        }
    } else {
        Theme {
            dark: false,
            text: 0xe4000000,
            secondary: 0x9e000000,
            hover: 0x0a000000,
            pressed: 0x06000000,
            selected: 0x0f000000,
            divider: 0x0f000000,
            accent: 0xff0067c0,
            text_ref: 0x001a1a1a,
            bg_ref: 0x00f3f3f3,
        }
    }
}

/// Effective DPI scale of the monitor that contains `point`.
pub unsafe fn scale_at(point: POINT) -> f32 {
    let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
    let (mut x, mut y) = (96u32, 96u32);
    if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) != 0 {
        x = 96;
    }
    x as f32 / 96.0
}

/// Gives a popup window rounded corners and the acrylic backdrop used by
/// transient Windows 11 surfaces.
pub unsafe fn acrylic(hwnd: HWND, dark: bool) {
    let dark: BOOL = dark.into();
    let corner = DWMWCP_ROUND;
    let backdrop = DWMSBT_TRANSIENTWINDOW;
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
        (&dark as *const BOOL).cast(),
        4,
    );
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
        (&corner as *const i32).cast(),
        4,
    );
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_SYSTEMBACKDROP_TYPE as u32,
        (&backdrop as *const i32).cast(),
        4,
    );
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    DwmExtendFrameIntoClientArea(hwnd, &margins);
}

/// Makes show and hide immediate, so a hidden window never appears in a
/// capture taken right afterwards.
pub unsafe fn disable_transitions(hwnd: HWND) {
    let value: BOOL = 1;
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_TRANSITIONS_FORCEDISABLED as u32,
        (&value as *const BOOL).cast(),
        4,
    );
}

/// Puts a topmost window above the other topmost windows without activating it.
/// Closing the snipping overlay activates the window used before the snip, and
/// if that one is always on top, its own thread raises it a moment later over
/// the windows shown since; they call this again after that.
pub unsafe fn raise(hwnd: HWND) {
    SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
}

/// Whether the DWM Mica backdrop exists (Windows 11 22H2 or later).
pub fn supports_mica() -> bool {
    let mut buffer = [0u16; 16];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: the buffer and its byte size describe `buffer`.
    let found = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion").as_ptr(),
            w("CurrentBuildNumber").as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    } == 0;
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    found
        && String::from_utf16_lossy(&buffer[..length])
            .parse::<u32>()
            .is_ok_and(|build| build >= 22621)
}

/// Draws the Mica material behind the whole window, including the title bar.
pub unsafe fn mica(hwnd: HWND, dark: bool) {
    let dark: BOOL = dark.into();
    let backdrop = DWMSBT_MAINWINDOW;
    let default_color = 0xffff_ffffu32; // DWMWA_COLOR_DEFAULT
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
        (&dark as *const BOOL).cast(),
        4,
    );
    for attribute in [34u32, 35] {
        DwmSetWindowAttribute(hwnd, attribute, (&default_color as *const u32).cast(), 4);
    }
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_SYSTEMBACKDROP_TYPE as u32,
        (&backdrop as *const i32).cast(),
        4,
    );
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    DwmExtendFrameIntoClientArea(hwnd, &margins);
}

/// Colors the title bar to match the window background (Windows 11).
pub unsafe fn window_theme(hwnd: HWND, theme: &Theme) {
    let dark: BOOL = theme.dark.into();
    DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
        (&dark as *const BOOL).cast(),
        4,
    );
    for attribute in [34u32, 35] {
        // DWMWA_BORDER_COLOR and DWMWA_CAPTION_COLOR
        DwmSetWindowAttribute(hwnd, attribute, (&theme.bg_ref as *const u32).cast(), 4);
    }
    DwmSetWindowAttribute(
        hwnd,
        36, // DWMWA_TEXT_COLOR
        (&theme.text_ref as *const u32).cast(),
        4,
    );
}

pub unsafe fn cloak(hwnd: HWND, cloaked: bool) {
    let value: BOOL = cloaked.into();
    DwmSetWindowAttribute(hwnd, DWMWA_CLOAK as u32, (&value as *const BOOL).cast(), 4);
}

pub struct Font(pub HFONT);
impl Font {
    pub unsafe fn new(face: &str, pixels: f32, weight: i32) -> Font {
        Font(CreateFontW(
            -(pixels.round() as i32),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            ANTIALIASED_QUALITY as u32,
            DEFAULT_PITCH as u32,
            w(face).as_ptr(),
        ))
    }
}
impl Drop for Font {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the font was created by `Font::new` and is not selected.
            unsafe { DeleteObject(self.0) };
        }
    }
}

/// Downscales straight-alpha BGRA pixels with an area filter and returns
/// premultiplied pixels.
pub fn thumbnail(pixels: &[u8], width: i32, height: i32, tw: i32, th: i32) -> Vec<u8> {
    let (width, height, tw, th) = (width as usize, height as usize, tw as usize, th as usize);
    let mut out = vec![0u8; tw * th * 4];
    if width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return out;
    }
    for ty in 0..th {
        let y0 = ty * height / th;
        let y1 = ((ty + 1) * height / th).max(y0 + 1).min(height);
        for tx in 0..tw {
            let x0 = tx * width / tw;
            let x1 = ((tx + 1) * width / tw).max(x0 + 1).min(width);
            let mut sum = [0u64; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = &pixels[(y * width + x) * 4..][..4];
                    let a = p[3] as u64;
                    sum[0] += p[0] as u64 * a;
                    sum[1] += p[1] as u64 * a;
                    sum[2] += p[2] as u64 * a;
                    sum[3] += a;
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u64;
            let o = &mut out[(ty * tw + tx) * 4..][..4];
            for c in 0..3 {
                o[c] = (sum[c] / (255 * n)) as u8;
            }
            o[3] = (sum[3] / n) as u8;
        }
    }
    out
}

/// Returns the capture-frame-and-scissors icon as straight-alpha RGBA.
/// The shared bitmap and executable icon are generated from app.svg together.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    const RGBA: &[u8; 64 * 64 * 4] = include_bytes!("app.rgba");
    if size == 64 {
        return RGBA.to_vec();
    }
    // The area filter is channel-order agnostic and returns premultiplied
    // pixels; egui and CreateIcon require straight-alpha colors instead.
    let mut pixels = thumbnail(RGBA, 64, 64, size as i32, size as i32);
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = (u32::from(*channel) * 255 + alpha / 2)
                .checked_div(alpha)
                .unwrap_or(0)
                .min(255) as u8;
        }
    }
    pixels
}

/// Loads an owned executable icon; the caller destroys it when no longer needed.
pub unsafe fn app_icon(size: u32) -> HICON {
    LoadImageW(
        GetModuleHandleW(null()),
        1usize as windows_sys::core::PCWSTR,
        IMAGE_ICON,
        size as i32,
        size as i32,
        0,
    )
}

pub fn rect(left: f32, top: f32, right: f32, bottom: f32) -> RECT {
    RECT {
        left: left.round() as i32,
        top: top.round() as i32,
        right: right.round() as i32,
        bottom: bottom.round() as i32,
    }
}

/// Whether `p` lies in `[left, top, right, bottom]`; the right and bottom edges are outside.
pub fn contains([left, top, right, bottom]: [f32; 4], p: POINT) -> bool {
    let (x, y) = (p.x as f32, p.y as f32);
    x >= left && x < right && y >= top && y < bottom
}

#[cfg(test)]
mod tests;
