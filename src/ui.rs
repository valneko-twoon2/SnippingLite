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
            FindWindowExW, GetPropW, GetWindowThreadProcessId, LoadImageW, SetPropW, HICON,
            IMAGE_ICON,
        },
    },
};

use crate::w;

pub const ICON_FONT: &str = "Segoe Fluent Icons";
pub const TEXT_FONT: &str = "Yu Gothic UI";

/// Finds a document by role even when settings and history share a caption.
pub unsafe fn find_app_window(identity: &str) -> HWND {
    let mut previous = null_mut();
    loop {
        let window = FindWindowExW(
            null_mut(),
            previous,
            null(),
            w(crate::SETTINGS_TITLE).as_ptr(),
        );
        if window.is_null() || !GetPropW(window, w(identity).as_ptr()).is_null() {
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
        let mut process = 0;
        GetWindowThreadProcessId(window, &mut process);
        if process == std::process::id() {
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

pub fn theme() -> Theme {
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
    if found && light == 0 {
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

/// A premultiplied, top-down 32-bit DIB with its own memory DC.
pub struct Canvas {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    pub width: i32,
    pub height: i32,
}

impl Canvas {
    pub fn app_icon(&mut self, x: i32, y: i32, size: u32) {
        let rgba = icon_rgba(size);
        let width = self.width;
        let height = self.height;
        let target = self.pixels();
        for row in 0..size as i32 {
            for column in 0..size as i32 {
                let (tx, ty) = (x + column, y + row);
                if tx < 0 || ty < 0 || tx >= width || ty >= height {
                    continue;
                }
                let i = ((row * size as i32 + column) * 4) as usize;
                let color = u32::from_be_bytes([rgba[i + 3], rgba[i], rgba[i + 1], rgba[i + 2]]);
                let dest = (ty * width + tx) as usize;
                target[dest] = over(target[dest], color, 1.0);
            }
        }
    }

    /// Paints a stable Fluent surface instead of inheriting DWM's inactive tint.
    pub fn surface(&mut self, dark: bool, scale: f32) {
        self.pixels()
            .fill(if dark { 0xff202020 } else { 0xfff3f3f3 });
        let border = if dark { 0xff414141 } else { 0xffd0d0d0 };
        let fill = if dark { 0xff202020 } else { 0xfff3f3f3 };
        let bounds = [0.0, 0.0, self.width as f32, self.height as f32];
        self.fill_round_rect(bounds, 8.0 * scale, border);
        self.fill_round_rect(
            [scale, scale, bounds[2] - scale, bounds[3] - scale],
            7.0 * scale,
            fill,
        );
    }

    pub unsafe fn new(width: i32, height: i32) -> Option<Canvas> {
        if width < 1 || height < 1 {
            return None;
        }
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let dc = CreateCompatibleDC(null_mut());
        if dc.is_null() {
            return None;
        }
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            DeleteDC(dc);
            return None;
        }
        let old = SelectObject(dc, bitmap);
        Some(Canvas {
            dc,
            bitmap,
            old,
            bits: bits.cast(),
            width,
            height,
        })
    }

    pub fn dc(&self) -> HDC {
        self.dc
    }

    pub fn pixels(&mut self) -> &mut [u32] {
        // SAFETY: the DIB section holds exactly width * height pixels and
        // lives as long as `self`.
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.width * self.height) as usize) }
    }

    /// Fills an anti-aliased rounded rectangle given in pixels.
    pub fn fill_round_rect(&mut self, rect: [f32; 4], radius: f32, color: u32) {
        let (width, height) = (self.width, self.height);
        let [left, top, right, bottom] = rect;
        let (cx, cy) = ((left + right) / 2.0, (top + bottom) / 2.0);
        let (hw, hh) = ((right - left) / 2.0, (bottom - top) / 2.0);
        let radius = radius.min(hw).min(hh).max(0.0);
        let x0 = (left.floor() as i32).clamp(0, width);
        let x1 = (right.ceil() as i32).clamp(0, width);
        let y0 = (top.floor() as i32).clamp(0, height);
        let y1 = (bottom.ceil() as i32).clamp(0, height);
        let pixels = self.pixels();
        for y in y0..y1 {
            for x in x0..x1 {
                let qx = (x as f32 + 0.5 - cx).abs() - (hw - radius);
                let qy = (y as f32 + 0.5 - cy).abs() - (hh - radius);
                let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
                let distance = outside + qx.max(qy).min(0.0) - radius;
                let coverage = (0.5 - distance).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    let i = (y * width + x) as usize;
                    pixels[i] = over(pixels[i], color, coverage);
                }
            }
        }
    }

    /// Draws text with grayscale anti-aliasing. `rect` is in pixels and
    /// `format` takes `DrawTextW` flags.
    pub unsafe fn text(&mut self, text: &str, font: &Font, rect: RECT, format: u32, color: u32) {
        // An empty Vec<u16> has a dangling pointer; DrawTextW can still inspect
        // its first character even with a zero count.
        if text.is_empty() {
            return;
        }
        let left = rect.left.clamp(0, self.width);
        let top = rect.top.clamp(0, self.height);
        let right = rect.right.clamp(0, self.width);
        let bottom = rect.bottom.clamp(0, self.height);
        let Some(mut mask) = Canvas::new(right - left, bottom - top) else {
            return;
        };
        let old = SelectObject(mask.dc, font.0);
        SetBkMode(mask.dc, TRANSPARENT as i32);
        SetTextColor(mask.dc, 0x00ffffff);
        let mut area = RECT {
            left: rect.left - left,
            top: rect.top - top,
            right: rect.right - left,
            bottom: rect.bottom - top,
        };
        let mut s: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(mask.dc, s.as_mut_ptr(), s.len() as i32, &mut area, format);
        SelectObject(mask.dc, old);
        GdiFlush();
        let mask_width = mask.width;
        let width = self.width;
        let coverage = mask.pixels();
        let target = self.pixels();
        for y in 0..(bottom - top) {
            for x in 0..(right - left) {
                let c = (coverage[(y * mask_width + x) as usize] >> 8) & 0xff;
                if c != 0 {
                    let i = ((y + top) * width + x + left) as usize;
                    target[i] = over(target[i], color, c as f32 / 255.0);
                }
            }
        }
    }

    pub unsafe fn present(&self, dc: HDC) {
        BitBlt(dc, 0, 0, self.width, self.height, self.dc, 0, 0, SRCCOPY);
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        // SAFETY: the objects were created in `Canvas::new`.
        unsafe {
            SelectObject(self.dc, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

/// Composites a straight-alpha color with extra coverage over a premultiplied pixel.
fn over(dst: u32, color: u32, coverage: f32) -> u32 {
    let a = ((color >> 24) as f32 * coverage).round() as u32;
    let premultiply = |shift: u32| (((color >> shift) & 0xff) * a + 127) / 255;
    blend(
        dst,
        a << 24 | premultiply(16) << 16 | premultiply(8) << 8 | premultiply(0),
    )
}

/// Premultiplied source-over.
fn blend(dst: u32, src: u32) -> u32 {
    let inverse = 255 - (src >> 24);
    let channel = |shift: u32| {
        let d = (dst >> shift) & 0xff;
        let s = (src >> shift) & 0xff;
        (s + (d * inverse + 127) / 255).min(255) << shift
    };
    channel(24) | channel(16) | channel(8) | channel(0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_roles_distinguish_windows_with_the_same_caption() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, WS_POPUP,
        };
        // SAFETY: these are private, hidden test windows using the system STATIC class.
        unsafe {
            let create = || {
                CreateWindowExW(
                    0,
                    w("STATIC").as_ptr(),
                    w(crate::SETTINGS_TITLE).as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null(),
                )
            };
            let settings = create();
            let history = create();
            assert!(!settings.is_null() && !history.is_null());
            SetPropW(
                settings,
                w("SnippingLite.Test.Settings").as_ptr(),
                1usize as HANDLE,
            );
            SetPropW(
                history,
                w("SnippingLite.Test.History").as_ptr(),
                1usize as HANDLE,
            );
            let found_settings = find_app_window("SnippingLite.Test.Settings");
            let found_history = find_app_window("SnippingLite.Test.History");
            DestroyWindow(settings);
            DestroyWindow(history);
            assert_eq!(found_settings, settings);
            assert_eq!(found_history, history);
        }
    }

    #[test]
    fn empty_notification_text_leaves_the_canvas_unchanged() {
        // SAFETY: all GDI objects are local and the text bounds fit the canvas.
        unsafe {
            let mut canvas = Canvas::new(120, 24).expect("Create the notification canvas");
            canvas.pixels().fill(0xff202020);
            let font = Font::new(TEXT_FONT, 14.0, FW_NORMAL as i32);
            canvas.text(
                "",
                &font,
                rect(0.0, 0.0, 120.0, 24.0),
                DT_VCENTER | DT_SINGLELINE,
                0xffffffff,
            );
            assert!(canvas.pixels().iter().all(|pixel| *pixel == 0xff202020));
        }
    }

    #[test]
    fn thumbnail_averages_and_premultiplies() {
        // Two opaque pixels (white and black) and two transparent ones.
        let pixels = [
            255, 255, 255, 255, 0, 0, 0, 255, //
            255, 255, 255, 0, 255, 255, 255, 0,
        ];
        assert_eq!(thumbnail(&pixels, 2, 2, 1, 1), vec![63, 63, 63, 127]);
        assert_eq!(
            thumbnail(&pixels, 2, 2, 2, 1),
            vec![127, 127, 127, 127, 0, 0, 0, 127]
        );
    }

    #[test]
    fn blend_keeps_premultiplied_range() {
        assert_eq!(blend(0xffffffff, 0x80000000), 0xff7f7f7f);
        assert_eq!(over(0, 0xffffffff, 0.5), 0x80808080);
    }
}
