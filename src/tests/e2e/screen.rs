use super::*;

/// The screen pixels of `r` as 0xRRGGBB, top-down.
pub fn screen(r: RECT) -> Vec<u32> {
    let (width, height) = (r.right - r.left, r.bottom - r.top);
    // SAFETY: the bitmap is sized for the area and all GDI objects are released here.
    unsafe {
        let screen = GetDC(null_mut());
        let dc = CreateCompatibleDC(screen);
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        assert!(
            !bitmap.is_null() && !bits.is_null(),
            "Create a capture bitmap"
        );
        let old = SelectObject(dc, bitmap);
        BitBlt(
            dc,
            0,
            0,
            width,
            height,
            screen,
            r.left,
            r.top,
            SRCCOPY | CAPTUREBLT,
        );
        GdiFlush();
        let pixels = std::slice::from_raw_parts(bits.cast::<u32>(), (width * height) as usize)
            .iter()
            .map(|p| p & 0xffffff)
            .collect();
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(null_mut(), screen);
        pixels
    }
}

/// Whether something other than the plain or dimmed backdrop covers most of `r`.
pub fn covered(r: RECT) -> bool {
    let inner = RECT {
        left: r.left + 4,
        top: r.top + 4,
        right: r.right - 4,
        bottom: r.bottom - 4,
    };
    let pixels = screen(inner);
    let other = pixels
        .iter()
        .filter(|&&p| !near(p, BACKGROUND, 12) && !near(p, DIMMED, 12))
        .count();
    other * 10 > pixels.len() * 4
}

/// Whether the snipping overlay dims the backdrop around `p`.
pub fn dimmed(p: POINT) -> bool {
    screen(RECT {
        left: p.x,
        top: p.y,
        right: p.x + 4,
        bottom: p.y + 4,
    })
    .iter()
    .all(|&c| near(c, DIMMED, 12))
}

/// The bounds of every pixel of exactly `rgb` on all monitors.
pub fn find(rgb: u32) -> Option<RECT> {
    // SAFETY: plain metrics queries.
    let desktop = unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        RECT {
            left,
            top,
            right: left + GetSystemMetrics(SM_CXVIRTUALSCREEN),
            bottom: top + GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    };
    let width = (desktop.right - desktop.left) as usize;
    let mut bounds: Option<RECT> = None;
    for (i, _) in screen(desktop)
        .iter()
        .enumerate()
        .filter(|(_, &p)| p == rgb)
    {
        let (x, y) = (
            desktop.left + (i % width) as i32,
            desktop.top + (i / width) as i32,
        );
        let b = bounds.get_or_insert(RECT {
            left: x,
            top: y,
            right: x + 1,
            bottom: y + 1,
        });
        b.left = b.left.min(x);
        b.top = b.top.min(y);
        b.right = b.right.max(x + 1);
        b.bottom = b.bottom.max(y + 1);
    }
    bounds
}

/// Opens the clipboard; another app may hold it for a moment.
pub fn open_clipboard() -> bool {
    for _ in 0..40 {
        // SAFETY: opening without an owner window is allowed.
        if unsafe { OpenClipboard(null_mut()) } != 0 {
            return true;
        }
        settle(25);
    }
    false
}

pub fn clear_clipboard() {
    assert!(open_clipboard(), "Open the clipboard");
    // SAFETY: the clipboard is open.
    unsafe {
        EmptyClipboard();
        CloseClipboard();
    }
}

/// The clipboard image (the app's 32-bit DIB) as top-down BGRA pixels.
pub fn clipboard_image() -> Option<(i32, i32, Vec<u8>)> {
    if !open_clipboard() {
        return None;
    }
    // SAFETY: the data is only read while locked and within its allocation size.
    unsafe {
        let handle = GetClipboardData(17);
        let mut image = None;
        let data = if handle.is_null() {
            null_mut()
        } else {
            GlobalLock(handle).cast::<u8>()
        };
        if !data.is_null() {
            let header = std::ptr::read_unaligned(data.cast::<BITMAPV5HEADER>());
            let (width, height) = (header.bV5Width, header.bV5Height);
            let row = width.max(0) as usize * 4;
            let length = row * height.unsigned_abs() as usize;
            if header.bV5Size as usize + length <= GlobalSize(handle) {
                let bytes = std::slice::from_raw_parts(data.add(header.bV5Size as usize), length);
                let pixels: Vec<u8> = if height > 0 {
                    bytes.chunks_exact(row).rev().flatten().copied().collect()
                } else {
                    bytes.to_vec()
                };
                image = Some((width, height.abs(), pixels));
            }
            GlobalUnlock(handle);
        }
        CloseClipboard();
        image
    }
}

pub fn clipboard_text() -> Option<String> {
    if !open_clipboard() {
        return None;
    }
    // SAFETY: the text is read up to its terminator within the allocation while locked.
    unsafe {
        let handle = GetClipboardData(13);
        let mut text = None;
        let data = if handle.is_null() {
            null_mut()
        } else {
            GlobalLock(handle).cast::<u16>()
        };
        if !data.is_null() {
            let units = std::slice::from_raw_parts(data, GlobalSize(handle) / 2);
            let length = units.iter().position(|&u| u == 0).unwrap_or(units.len());
            text = Some(String::from_utf16_lossy(&units[..length]));
            GlobalUnlock(handle);
        }
        CloseClipboard();
        text
    }
}

/// The color and alpha of a pixel of top-down BGRA pixels.
pub fn pixel(pixels: &[u8], width: i32, x: i32, y: i32) -> (u32, u8) {
    let p = &pixels[(y * width + x) as usize * 4..][..4];
    (
        (u32::from(p[2]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[0]),
        p[3],
    )
}

/// The size in a PNG file's header.
pub fn png_size(path: &Path) -> Option<(u32, u32)> {
    let data = std::fs::read(path).ok()?;
    if data.get(..8)? != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let number = |at: usize| Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?));
    Some((number(16)?, number(20)?))
}

/// The length of a finished MP4 file from its movie header; `None` while it is still recorded.
pub fn mp4_seconds(path: &Path) -> Option<f64> {
    let data = std::fs::read(path).ok()?;
    if data.get(4..8)? != b"ftyp" {
        return None;
    }
    let at = data.windows(4).position(|w| w == b"mvhd")? + 4;
    let number = |at: usize, size: usize| {
        data.get(at..at + size)
            .map(|b| b.iter().fold(0u64, |n, &byte| (n << 8) | u64::from(byte)))
    };
    let (scale, duration) = if *data.get(at)? == 1 {
        (number(at + 20, 4)?, number(at + 24, 8)?)
    } else {
        (number(at + 12, 4)?, number(at + 16, 4)?)
    };
    (scale > 0).then(|| duration as f64 / scale as f64)
}
