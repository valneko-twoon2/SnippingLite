use super::*;

pub fn send_mouse(flags: MOUSE_EVENT_FLAGS) {
    // SAFETY: one fully initialized input record.
    unsafe {
        let mut input: INPUT = std::mem::zeroed();
        input.r#type = INPUT_MOUSE;
        input.Anonymous.mi.dwFlags = flags;
        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn move_to(p: POINT) {
    // SAFETY: moving the cursor has no memory effects.
    unsafe { SetCursorPos(p.x, p.y) };
}

pub fn click(p: POINT) {
    move_to(p);
    settle(150);
    send_mouse(MOUSEEVENTF_LEFTDOWN);
    settle(60);
    send_mouse(MOUSEEVENTF_LEFTUP);
    settle(500);
}

/// Presses the left button at the first point, moves through the others and lets go at the last.
pub fn drag(path: &[POINT]) {
    move_to(path[0]);
    settle(150);
    send_mouse(MOUSEEVENTF_LEFTDOWN);
    settle(150);
    for pair in path.windows(2) {
        for step in 1..=12 {
            move_to(POINT {
                x: pair[0].x + (pair[1].x - pair[0].x) * step / 12,
                y: pair[0].y + (pair[1].y - pair[0].y) * step / 12,
            });
            settle(15);
        }
    }
    settle(100);
    send_mouse(MOUSEEVENTF_LEFTUP);
    settle(800);
}

pub fn select(from: POINT, width: i32, height: i32) {
    drag(&[
        from,
        POINT {
            x: from.x + width,
            y: from.y + height,
        },
    ]);
}

/// Presses the keys together and releases them in reverse.
pub fn keys(keys: &[VIRTUAL_KEY]) {
    let input = |key: VIRTUAL_KEY, up: bool| {
        // SAFETY: a fully initialized keyboard input record.
        unsafe {
            let mut input: INPUT = std::mem::zeroed();
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki.wVk = key;
            input.Anonymous.ki.dwFlags = if up { KEYEVENTF_KEYUP } else { 0 };
            SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
        }
    };
    for &key in keys {
        input(key, false);
        settle(20);
    }
    for &key in keys.iter().rev() {
        input(key, true);
        settle(20);
    }
    settle(250);
}

pub fn shortcut() {
    keys(&[VK_MENU, VK_SHIFT, 0x53]);
    settle(800);
}
