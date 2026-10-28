//! GDI capture of a desktop area, with the cursor and freeform mask applied.

use super::*;
use std::ops::Range;
use windows_sys::Win32::{
    Graphics::Gdi::*,
    UI::WindowsAndMessaging::{
        DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, DI_NORMAL, HCURSOR,
        ICONINFO,
    },
};

/// Cursor hotspot metadata is cached until the cursor changes. GetIconInfo
/// allocates two GDI bitmaps, so calling it for every frame is unnecessary.
struct Cursor {
    icon: HCURSOR,
    hotspot: POINT,
}

impl Cursor {
    unsafe fn draw(&mut self, dc: HDC, area: &RECT) {
        let mut info: CURSORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<CURSORINFO>() as u32;
        if GetCursorInfo(&mut info) == 0 || info.flags & CURSOR_SHOWING == 0 {
            return;
        }
        if self.icon != info.hCursor {
            let mut icon: ICONINFO = std::mem::zeroed();
            if GetIconInfo(info.hCursor, &mut icon) == 0 {
                return;
            }
            self.icon = info.hCursor;
            self.hotspot = POINT {
                x: icon.xHotspot as i32,
                y: icon.yHotspot as i32,
            };
            if !icon.hbmMask.is_null() {
                DeleteObject(icon.hbmMask);
            }
            if !icon.hbmColor.is_null() {
                DeleteObject(icon.hbmColor);
            }
        }
        DrawIconEx(
            dc,
            info.ptScreenPos.x - self.hotspot.x - area.left,
            info.ptScreenPos.y - self.hotspot.y - area.top,
            info.hCursor,
            0,
            0,
            0,
            null_mut(),
            DI_NORMAL,
        );
    }
}

/// Precomputes byte ranges outside an even-odd polygon. MP4 has no alpha, so
/// each frame fills these ranges with black after drawing the cursor.
pub(super) fn outside_runs(width: i32, height: i32, points: &[POINT]) -> Vec<Range<usize>> {
    let mut outside = Vec::new();
    if points.len() < 3 {
        return outside;
    }
    let mut intersections = Vec::new();
    for y in 0..height {
        intersections.clear();
        let scan = f64::from(y) + 0.5;
        let mut previous = points[points.len() - 1];
        for current in points {
            let (ay, by) = (f64::from(previous.y), f64::from(current.y));
            if (ay > scan) != (by > scan) {
                let x = f64::from(previous.x)
                    + (scan - ay) * (f64::from(current.x) - f64::from(previous.x)) / (by - ay);
                intersections.push(x);
            }
            previous = *current;
        }
        intersections.sort_by(f64::total_cmp);
        let row = (y as usize * width as usize) * 4;
        let mut covered_until = 0;
        for span in intersections.as_chunks::<2>().0 {
            let left = ((span[0] - 0.5).ceil() as i32).clamp(0, width);
            let right = ((span[1] - 0.5).ceil() as i32).clamp(0, width);
            if left > covered_until {
                outside.push(row + covered_until as usize * 4..row + left as usize * 4);
            }
            covered_until = right;
        }
        if covered_until < width {
            outside.push(row + covered_until as usize * 4..row + width as usize * 4);
        }
    }
    outside
}

/// Screen device context and a top-down DIB section that frames are copied into.
pub(super) struct Grabber {
    screen: HDC,
    mem: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    area: RECT,
    length: usize,
    outside: Vec<Range<usize>>,
    cursor: Option<Cursor>,
}

impl Grabber {
    pub(super) unsafe fn new(area: RECT, points: &[POINT], cursor: bool) -> Option<Grabber> {
        let (width, height) = (area.right - area.left, area.bottom - area.top);
        let screen = GetDC(null_mut());
        let mem = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(mem, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        let old = if bitmap.is_null() || mem.is_null() {
            null_mut()
        } else {
            SelectObject(mem, bitmap)
        };
        let grabber = Grabber {
            screen,
            mem,
            bitmap,
            old,
            bits: bits.cast(),
            area,
            length: width as usize * height as usize * 4,
            outside: outside_runs(width, height, points),
            cursor: cursor.then_some(Cursor {
                icon: null_mut(),
                hotspot: POINT { x: 0, y: 0 },
            }),
        };
        // Dropping releases whatever was created.
        (!screen.is_null() && !mem.is_null() && !bitmap.is_null() && !bits.is_null())
            .then_some(grabber)
    }

    /// Copies the area (with the cursor) and returns its BGRX pixels.
    pub(super) unsafe fn grab(&mut self) -> &[u8] {
        let (width, height) = (
            self.area.right - self.area.left,
            self.area.bottom - self.area.top,
        );
        BitBlt(
            self.mem,
            0,
            0,
            width,
            height,
            self.screen,
            self.area.left,
            self.area.top,
            SRCCOPY | CAPTUREBLT,
        );
        if let Some(cursor) = &mut self.cursor {
            cursor.draw(self.mem, &self.area);
        }
        GdiFlush();
        let pixels = std::slice::from_raw_parts_mut(self.bits, self.length);
        for run in &self.outside {
            pixels[run.clone()].fill(0);
        }
        pixels
    }
}

impl Drop for Grabber {
    fn drop(&mut self) {
        // SAFETY: each handle is released at most once and only when it was created.
        unsafe {
            if !self.mem.is_null() {
                if !self.old.is_null() {
                    SelectObject(self.mem, self.old);
                }
                DeleteDC(self.mem);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.screen.is_null() {
                ReleaseDC(null_mut(), self.screen);
            }
        }
    }
}
