use super::*;

/// 折れ線の途中の頂点で止まる時間（ミリ秒）。ポインターが頂点を一瞬で通り過ぎると、アプリが
/// 描画などで忙しい間のマウスの移動はまとめて 1 回で届き（最後の位置だけが残る）、頂点が
/// 自由形式の形から抜け落ちる。人が角で手を止めるように、アプリが頂点の位置を受け取るまで待つ。
const VERTEX_HOLD: u64 = 250;

/// テストが最後にカーソルを置いた位置。
static PLACED: Mutex<Option<POINT>> = Mutex::new(None);

/// ボタンを押す・離す直前に、カーソルがテストの置いた位置にあるか確かめ、なければ置き直す。
/// テストの外（机の上のマウスや、ほかのプロセス）が 1 ピクセルでも動かすと、選んだ範囲の大きさが
/// 変わる（録画できる最小の大きさの境目を確かめるテストは、それで結果が変わる）。動かされていたら
/// テストの外からの邪魔として数える（`focus.rs`）。
fn keep_cursor() {
    let Some(placed) = *PLACED.lock().unwrap_or_else(|e| e.into_inner()) else {
        return;
    };
    let mut now = POINT { x: 0, y: 0 };
    // SAFETY: plain cursor calls.
    unsafe {
        GetCursorPos(&mut now);
        if (now.x, now.y) != (placed.x, placed.y) {
            report(format_args!(
                "カーソルが {},{} から {},{} へ動かされていたので置き直す",
                placed.x, placed.y, now.x, now.y
            ));
            note_disturbance();
            SetCursorPos(placed.x, placed.y);
        }
    }
}

pub fn send_mouse(flags: MOUSE_EVENT_FLAGS) {
    // 押す前に、前面とポインターの下がこのテストかアプリの窓か確かめる。離す前も前面を
    // 確かめる（管理者として動くアプリが前面だと、離したことが捨てられて押したままになる）が、
    // 取り戻せなくても必ず送る（ボタンを押したままにしない）。カーソルの位置は前面を確かめてから
    // 確かめる（前面が管理者として動くアプリのままだと、置き直せない。`move_to`）。
    if flags & MOUSEEVENTF_LEFTDOWN != 0 {
        claim_foreground();
        keep_cursor();
        assert_ours_under_cursor();
    } else {
        let _ = reclaim_foreground();
        keep_cursor();
    }
    // SAFETY: one fully initialized input record.
    unsafe {
        let mut input: INPUT = std::mem::zeroed();
        input.r#type = INPUT_MOUSE;
        input.Anonymous.mi.dwFlags = flags;
        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
    }
}

/// カーソルを置く。前面の窓が管理者として動くアプリ（タスク マネージャーなど）のものだと、Windows は
/// カーソルを動かさない（UIPI）。ほかの作業の端末の窓が閉じた後などにそうなるので、動かせなければ
/// 前面を取り戻してから置き直す。置き直せなくても、ボタンを押す前に `keep_cursor` が確かめる。
pub fn move_to(p: POINT) {
    *PLACED.lock().unwrap_or_else(|e| e.into_inner()) = Some(p);
    // SAFETY: moving the cursor has no memory effects.
    if unsafe { SetCursorPos(p.x, p.y) } == 0 {
        let _ = reclaim_foreground();
        // SAFETY: as above.
        unsafe { SetCursorPos(p.x, p.y) };
    }
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
    for (index, pair) in path.windows(2).enumerate() {
        for step in 1..=12 {
            move_to(POINT {
                x: pair[0].x + (pair[1].x - pair[0].x) * step / 12,
                y: pair[0].y + (pair[1].y - pair[0].y) * step / 12,
            });
            settle(15);
        }
        // 最後の点は下の待ちと離すときの位置で届く。途中の頂点ではここで止まる。
        if index + 2 < path.len() {
            settle(VERTEX_HOLD);
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
    // キーを 1 つ押すたびに、前面がこのテストかアプリの窓か確かめる（押している途中でほかの
    // アプリの窓が前面に出ても、そこへキーを送らない）。取り戻せなければ、押したキーを離して
    // から止める。離す前も確かめるが、取り戻せなくても離すことは必ず送る。
    let mut pressed = 0;
    let mut failure = None;
    for &key in keys {
        if let Err(message) = reclaim_foreground() {
            failure = Some(message);
            break;
        }
        input(key, false);
        pressed += 1;
        settle(20);
    }
    for &key in keys[..pressed].iter().rev() {
        let _ = reclaim_foreground();
        input(key, true);
        settle(20);
    }
    if let Some(message) = failure {
        panic!("{message}");
    }
    settle(250);
}

/// ショートカットを押す前にポインターを置く、主モニターの上の決まった位置。アプリは撮影の画面の
/// ツールバーを、ポインターのあるモニターの上端の中央に、そのモニターの倍率で出す。テストは主モニターの
/// ツールバーを見るので、ポインターがほかのモニター（主モニターの左など）にあると、ツールバーが
/// 見つからない。位置は、ツールバー、選ぶ範囲、見本の板（`PANEL`）、確かめる点のどれにも重ならない、
/// 左下の 4 分の 1 の角にする。
fn resting_point() -> POINT {
    let (_, r) = primary_monitor();
    POINT {
        x: r.left + (r.right - r.left) / 4,
        y: r.top + (r.bottom - r.top) * 3 / 4,
    }
}

pub fn shortcut() {
    move_to(resting_point());
    keys(&[VK_MENU, VK_SHIFT, 0x53]);
    settle(800);
}

/// テストの前のポインターの位置。テストの終わりに `restore` でそこへ戻す（`on_desktop`）。
pub struct CursorHome(pub POINT);

impl CursorHome {
    /// 今のポインターの位置を覚える。座標を物理的な画素で受け取るよう、`setup` の後に呼ぶ。
    pub fn remember() -> CursorHome {
        let mut p = POINT { x: 0, y: 0 };
        // SAFETY: a plain cursor query.
        unsafe { GetCursorPos(&mut p) };
        CursorHome(p)
    }

    /// 覚えた位置へポインターを戻し、戻せたかを返す。前面の窓が管理者として動くアプリ（タスク
    /// マネージャーなど）のものだと、Windows はポインターを動かさない（UIPI）ので、背景の窓を前面に
    /// 戻してから、背景の窓を閉じる前に呼ぶ。
    pub fn restore(&self) -> bool {
        // テストが置いた位置は、この後は確かめない。
        *PLACED.lock().unwrap_or_else(|e| e.into_inner()) = None;
        let _ = reclaim_foreground();
        // SAFETY: moving the cursor has no memory effects.
        unsafe { SetCursorPos(self.0.x, self.0.y) != 0 }
    }
}
