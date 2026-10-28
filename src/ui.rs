//! Drawing helpers for the Windows 11 style surfaces (toolbar and notification).
//!
//! The surfaces use a DWM acrylic backdrop, so everything is rendered into a
//! premultiplied 32-bit DIB and copied to the window in one `BitBlt`.

use std::ptr::null_mut;
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
    UI::{Controls::MARGINS, HiDpi::*},
};

use crate::w;

pub const ICON_FONT: &str = "Segoe Fluent Icons";
pub const TEXT_FONT: &str = "Segoe UI Variable Text";

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
    let mut accent = 0u32;
    let mut opaque: BOOL = 0;
    // SAFETY: both pointers refer to live locals.
    let accent = if unsafe { DwmGetColorizationColor(&mut accent, &mut opaque) } >= 0 {
        accent | 0xff000000
    } else {
        0xff0067c0
    };
    if found && light == 0 {
        Theme {
            dark: true,
            text: 0xffffffff,
            secondary: 0xc8ffffff,
            hover: 0x0fffffff,
            pressed: 0x0affffff,
            selected: 0x15ffffff,
            divider: 0x15ffffff,
            accent,
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
            accent,
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

    /// Copies premultiplied BGRA pixels to (x, y), blending over the canvas.
    pub fn image(&mut self, x: i32, y: i32, width: i32, height: i32, pixels: &[u8]) {
        let canvas_width = self.width;
        let canvas_height = self.height;
        let target = self.pixels();
        for row in 0..height {
            let ty = y + row;
            if ty < 0 || ty >= canvas_height {
                continue;
            }
            for column in 0..width {
                let tx = x + column;
                let source = ((row * width + column) * 4) as usize;
                if tx < 0 || tx >= canvas_width || source + 4 > pixels.len() {
                    continue;
                }
                let p = &pixels[source..source + 4];
                let color = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
                let i = (ty * canvas_width + tx) as usize;
                target[i] = blend(target[i], color);
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
