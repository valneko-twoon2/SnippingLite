//! The premultiplied DIB that toolbar and notification surfaces draw into.

use super::*;

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
pub(super) fn over(dst: u32, color: u32, coverage: f32) -> u32 {
    let a = ((color >> 24) as f32 * coverage).round() as u32;
    let premultiply = |shift: u32| (((color >> shift) & 0xff) * a + 127) / 255;
    blend(
        dst,
        a << 24 | premultiply(16) << 16 | premultiply(8) << 8 | premultiply(0),
    )
}

/// Premultiplied source-over.
pub(super) fn blend(dst: u32, src: u32) -> u32 {
    let inverse = 255 - (src >> 24);
    let channel = |shift: u32| {
        let d = (dst >> shift) & 0xff;
        let s = (src >> shift) & 0xff;
        (s + (d * inverse + 127) / 255).min(255) << shift
    };
    channel(24) | channel(16) | channel(8) | channel(0)
}
