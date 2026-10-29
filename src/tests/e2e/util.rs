use super::*;

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Handles the backdrop's messages for `ms` milliseconds; every wait goes through
/// here so the window keeps painting and answering the app's activation messages.
pub fn settle(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        // SAFETY: a plain message loop for windows of this thread.
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        if Instant::now() >= end {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Polls `check` until it gives a value or the time is up.
pub fn wait_for<T>(seconds: u64, mut check: impl FnMut() -> Option<T>) -> Option<T> {
    let end = Instant::now() + Duration::from_secs(seconds);
    loop {
        if let Some(value) = check() {
            return Some(value);
        }
        if Instant::now() >= end {
            return None;
        }
        settle(100);
    }
}

pub fn near(a: u32, b: u32, tolerance: u32) -> bool {
    (0..3).all(|i| ((a >> (i * 8)) & 0xff).abs_diff((b >> (i * 8)) & 0xff) <= tolerance)
}

pub fn colorref(rgb: u32) -> u32 {
    ((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff)
}

pub fn offset(r: RECT, x: i32, y: i32) -> RECT {
    RECT {
        left: r.left + x,
        top: r.top + y,
        right: r.right + x,
        bottom: r.bottom + y,
    }
}

pub fn center(r: RECT) -> POINT {
    POINT {
        x: (r.left + r.right) / 2,
        y: (r.top + r.bottom) / 2,
    }
}
