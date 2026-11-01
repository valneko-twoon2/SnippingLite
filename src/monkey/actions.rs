//! 操作の種類と重み。その時に実際に届く操作だけから、重みに比例して選ぶ。
//!
//! | 種類 | 届くとき | 重み |
//! |---|---|---|
//! | トレイの左クリック、ショートカット、NEW（3 つで 1 つの重み） | いつでも | 待機中は 12、カウントダウン中は 1、録画中は 0.4、ほかは 2.5 |
//! | トレイのメニュー（Snipping Lite、About、Escape） | ボタンを押していないとき | 2 |
//! | トレイへのコマンド（設定、履歴、About、録画の停止、WM_CLOSE） | いつでも | 2 |
//! | 無作為な設定を書いて TRAY_RELOAD | いつでも | 2.5 |
//! | オーバーレイ: 移動、押したまま、ドラッグ、クリック、ダブルクリック、右クリックとホイール | オーバーレイが出ているとき | 3、5、11、4、2.5、2.5（書き込み中は押したまま 5、ドラッグ 5、クリック 2.5。全画面のモードでは 1.5、3、1.5、1.5） |
//! | オーバーレイ: 離す | 押したままのとき | 40 |
//! | キー: Escape、Enter、Ctrl+Z/Y/C、そのほか | オーバーレイが出ているとき | 2.5（書き込み中は 1.5）、2.5（同 1.5）、2.5、3 |
//! | ツールバー: カメラとビデオ（ビデオを 6 割）、範囲のメニュー、ペン、履歴と歯車、×、移動・ホバー・ずらし（半分は移動） | ツールバーが出ているとき | 3.5、6、2.5、2、2、3 |
//! | 書き込みのバー: 道具、元に戻す・やり直す・すべて消去、キャプチャ、×、ホバー・ずらし・離れる | バーが出ているとき | 12、12、7、9、9 |
//! | 通知: ホバー・クリック・離れる、メニュー、タイマー（閉じ始める、滑らせる） | 通知が出ているとき | 12、7、8 |
//! | カウントダウン: 刻みと再描画、クリック | カウントダウンのとき（最後の刻みで録画が始まる） | 16、2 |
//! | 録画中の表示: 停止のボタン、表示のタイマーと停止でない所のクリック | 録画中 | 1、6 |
//! | システムのメッセージ（WM_DISPLAYCHANGE、WM_DPICHANGED、WM_SETTINGCHANGE、WM_CANCELMODE） | いつでも | 2 |
//! | 待つ（タイマーと描画を流す） | いつでも | 2 |
//!
//! 重みは 10 倍の整数で持つ。ボタンを押したままの間は、ほかのウィンドウへのマウスの操作は届かない。
//! 書き込みのバー、通知、カウントダウンは短い間しか出ないので、出ている間はその操作を重くする。
//! 録画はカウントダウンを 3 つ刻んでから始まるので、カウントダウン中は刻みを重く、取り消すクリックを
//! 軽くする（30 ケースで数回の録画になる）。録画中は止める手を軽くし、止めるまでに数手を送る。
//! 範囲のメニューは矩形、自由形式、ウィンドウ、全画面、Escape を 2:2:3:5:1 で選ぶ（ウィンドウと全画面の
//! モードは、的や画面を押すとすぐに撮れて終わるので多めにする。全画面は選ぶとその場で撮れるので、
//! 全画面の撮影中になるのは、その後に始めた撮影だけ。それでいちばん多くする）。

use super::*;
use crate::test_support::rng::Rng;

mod points;
use points::*;

/// オーバーレイかウィンドウの座標。
pub(super) type P = (i32, i32);

#[derive(Clone, Copy, Debug)]
pub(super) enum Start {
    Icon,
    Shortcut,
    New,
}

/// 通知の上の場所。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Spot {
    Close,
    Menu,
    Button,
    Text,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Sys {
    DisplayChange,
    DpiChanged,
    SettingChange,
    CancelMode,
}

#[derive(Clone, Debug)]
pub(super) enum Action {
    Start(Start),
    /// トレイのメニュー。`Some(0)` は「Snipping Lite」、`Some(1)` は About、`None` は Escape。
    TrayMenu(Option<usize>),
    /// トレイのウィンドウへのメッセージ（WM_COMMAND とその番号、または WM_CLOSE）。
    Command(u32, usize),
    Reload(Box<config::Settings>),
    Move(P),
    Hold(Vec<P>),
    Release(P),
    Drag(Vec<P>),
    Click(P),
    DoubleClick(P),
    RightClick(P),
    Wheel(P, i16),
    Key(u16, bool),
    Toolbar(i32),
    /// あるボタンで押し、別のボタンで離す。
    ToolbarSlip(i32, i32),
    ToolbarHover(i32),
    ToolbarLeave,
    /// 範囲のメニュー。`Some(m)` はモード m の項目、`None` は Escape。
    AreaMenu(Option<usize>),
    /// ボタンのない所でつかみ、左上をここ（画面の座標）へ動かす。
    MoveToolbar(P),
    Bar(i32),
    BarSlip(i32, i32),
    BarHover(i32),
    BarLeave,
    ToastHover(Spot),
    ToastClick(Spot),
    ToastLeave,
    ToastMenu(Option<usize>),
    /// 表示を終えて閉じ始めるタイマー。
    ToastDismiss,
    /// 滑らせるタイマー。滑り始めてから、この時間（ミリ秒）たったことにして送る。
    ToastSlide(u64),
    Tick,
    CountPaint,
    CountClick,
    /// 録画中の表示の停止のボタン。
    RecordStop,
    /// 録画中の表示の、停止のボタンでない所（左から x DIP）のクリック。
    RecordClick(f32),
    /// 録画中の表示のタイマー（枠を合わせ直し、表示を上げ直す）。
    RecordTick,
    System(Sys),
    Wait(u64),
}

/// 到達を数える、操作の種類の名前。
pub(super) const KINDS: [&str; 37] = [
    "tray: icon",
    "tray: shortcut",
    "tray: NEW",
    "tray: menu",
    "tray: command",
    "tray: new settings",
    "overlay: move",
    "overlay: press and hold",
    "overlay: release",
    "overlay: drag",
    "overlay: click",
    "overlay: double click",
    "overlay: right click or wheel",
    "key: Escape",
    "key: Enter",
    "key: Ctrl+Z, Y or C",
    "key: other",
    "toolbar: camera or video",
    "toolbar: area menu",
    "toolbar: pen",
    "toolbar: history or settings",
    "toolbar: close",
    "toolbar: move, hover or slip",
    "bar: tool",
    "bar: undo, redo or clear",
    "bar: capture",
    "bar: close",
    "bar: hover, slip or leave",
    "toast: pointer",
    "toast: menu",
    "toast: timer",
    "countdown: tick or paint",
    "countdown: click",
    "recording: stop button",
    "recording: tick or click elsewhere",
    "system message",
    "wait",
];

/// 到達を数える、手を選ぶ前の状態の名前。
pub(super) const STATES: [&str; 14] = [
    "idle",
    "idle with the kept toolbar",
    "snipping a rectangle",
    "snipping freeform",
    "snipping a window",
    "snipping the full screen",
    "snipping a recording",
    "selecting",
    "marking up",
    "countdown",
    "recording",
    "notification",
    "frozen",
    "live",
];

impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Action::Start(Start::Icon) => "tray: icon",
            Action::Start(Start::Shortcut) => "tray: shortcut",
            Action::Start(Start::New) => "tray: NEW",
            Action::TrayMenu(_) => "tray: menu",
            Action::Command(..) => "tray: command",
            Action::Reload(_) => "tray: new settings",
            Action::Move(_) => "overlay: move",
            Action::Hold(_) => "overlay: press and hold",
            Action::Release(_) => "overlay: release",
            Action::Drag(_) => "overlay: drag",
            Action::Click(_) => "overlay: click",
            Action::DoubleClick(_) => "overlay: double click",
            Action::RightClick(_) | Action::Wheel(..) => "overlay: right click or wheel",
            Action::Key(VK_ESCAPE, _) => "key: Escape",
            Action::Key(VK_RETURN, _) => "key: Enter",
            Action::Key(key, true) if b"ZYC".contains(&(*key as u8)) => "key: Ctrl+Z, Y or C",
            Action::Key(..) => "key: other",
            Action::Toolbar(CAMERA_BUTTON | VIDEO_BUTTON) => "toolbar: camera or video",
            Action::AreaMenu(_) => "toolbar: area menu",
            Action::Toolbar(EDIT_BUTTON) => "toolbar: pen",
            Action::Toolbar(HISTORY_BUTTON | SETTINGS_BUTTON) => "toolbar: history or settings",
            Action::Toolbar(_) => "toolbar: close",
            Action::ToolbarSlip(..)
            | Action::ToolbarHover(_)
            | Action::ToolbarLeave
            | Action::MoveToolbar(_) => "toolbar: move, hover or slip",
            Action::Bar(markup::probe::CAPTURE) => "bar: capture",
            Action::Bar(markup::probe::CLOSE) => "bar: close",
            Action::Bar(
                markup::probe::PEN | markup::probe::HIGHLIGHTER | markup::probe::ERASER,
            ) => "bar: tool",
            Action::Bar(_) => "bar: undo, redo or clear",
            Action::BarSlip(..) | Action::BarHover(_) | Action::BarLeave => {
                "bar: hover, slip or leave"
            }
            Action::ToastHover(_) | Action::ToastClick(_) | Action::ToastLeave => "toast: pointer",
            Action::ToastMenu(_) => "toast: menu",
            Action::ToastDismiss | Action::ToastSlide(_) => "toast: timer",
            Action::Tick | Action::CountPaint => "countdown: tick or paint",
            Action::CountClick => "countdown: click",
            Action::RecordStop => "recording: stop button",
            Action::RecordClick(_) | Action::RecordTick => "recording: tick or click elsewhere",
            Action::System(_) => "system message",
            Action::Wait(_) => "wait",
        }
    }
}

/// 選ぶときの種類。
#[derive(Clone, Copy)]
enum Kind {
    Start,
    TrayMenu,
    Command,
    Reload,
    Move,
    Hold,
    Release,
    Drag,
    Click,
    DoubleClick,
    OtherMouse,
    Escape,
    Enter,
    EditKey,
    OtherKey,
    Camera,
    AreaMenu,
    Pen,
    Leave,
    Close,
    Handle,
    BarTool,
    BarEdit,
    BarCapture,
    BarClose,
    BarPointer,
    ToastPointer,
    ToastMenu,
    ToastTimer,
    Tick,
    CountClick,
    RecordStop,
    RecordOther,
    System,
    Wait,
}

/// 今届く種類と、その重み（10 倍）。
fn choices(now: &Snapshot) -> Vec<(Kind, u32)> {
    let free = !now.held;
    let idle = !now.active && now.count.is_none();
    // 開始の手はカウントダウンを取り消し、録画を止めるので、そのときは軽くする。
    let start = match (idle, now.count.is_some(), now.recording) {
        (_, _, true) => 4,
        (true, _, _) => 120,
        (_, true, _) => 10,
        _ => 25,
    };
    let mut list = vec![
        (Kind::Start, start),
        (Kind::Command, 20),
        (Kind::Reload, 25),
        (Kind::System, 20),
        (Kind::Wait, 20),
    ];
    if free {
        list.push((Kind::TrayMenu, 20));
    }
    // 書き込み中は、終える手（Escape、Enter）とオーバーレイの上の手を軽くして、バーの手を重くする。
    let marking = now.session.is_some();
    if now.overlay {
        let ends = if marking { 15 } else { 25 };
        list.extend([
            (Kind::Move, 30),
            (Kind::Escape, ends),
            (Kind::Enter, ends),
            (Kind::EditKey, 25),
            (Kind::OtherKey, 30),
        ]);
        if free {
            // 全画面のモードは、どこを押してもすぐに撮れて終わるので、押す手を軽くする。
            let (hold, drag, click, double) = match (marking, now.mode == MODE_SCREEN) {
                (true, _) => (50, 50, 25, 25),
                (_, true) => (15, 30, 15, 15),
                _ => (50, 110, 40, 25),
            };
            list.extend([
                (Kind::Hold, hold),
                (Kind::Drag, drag),
                (Kind::Click, click),
                (Kind::DoubleClick, double),
                (Kind::OtherMouse, 25),
            ]);
        }
    }
    if now.held {
        list.push((Kind::Release, 400));
    }
    if free && now.toolbar.is_some() {
        list.extend([
            (Kind::Camera, 35),
            (Kind::AreaMenu, 60),
            (Kind::Pen, 25),
            (Kind::Leave, 20),
            (Kind::Close, 20),
            (Kind::Handle, 30),
        ]);
    }
    if free && now.bar {
        list.extend([
            (Kind::BarTool, 120),
            (Kind::BarEdit, 120),
            (Kind::BarCapture, 70),
            (Kind::BarClose, 90),
            (Kind::BarPointer, 90),
        ]);
    }
    if now.toast.is_some() {
        list.push((Kind::ToastTimer, 80));
        if free {
            list.extend([(Kind::ToastPointer, 120), (Kind::ToastMenu, 70)]);
        }
    }
    if now.count.is_some() {
        list.push((Kind::Tick, 160));
        if free {
            list.push((Kind::CountClick, 20));
        }
    }
    if now.recording {
        list.extend([(Kind::RecordStop, 10), (Kind::RecordOther, 60)]);
    }
    list
}

/// 次の操作。
pub(super) fn next(rng: &mut Rng, now: &Snapshot, scene: &Scene) -> Action {
    let list = choices(now);
    let weights: Vec<u32> = list.iter().map(|&(_, weight)| weight).collect();
    let kind = list[rng.weighted(&weights)].0;
    build(rng, kind, now, scene)
}

fn build(rng: &mut Rng, kind: Kind, now: &Snapshot, scene: &Scene) -> Action {
    match kind {
        Kind::Start => Action::Start(*rng.pick(&[Start::Icon, Start::Shortcut, Start::New])),
        Kind::TrayMenu => Action::TrayMenu(*rng.pick(&[Some(0), Some(1), None])),
        Kind::Command => {
            let (message, command) = *rng.pick(&[
                (WM_COMMAND, TRAY_SETTINGS),
                (WM_COMMAND, TRAY_HISTORY),
                (WM_COMMAND, TRAY_ABOUT),
                (WM_COMMAND, TRAY_STOP),
                (WM_CLOSE, 0),
            ]);
            Action::Command(message, command)
        }
        Kind::Reload => {
            let mut settings = config::load_settings();
            scene::randomize(rng, &mut settings);
            Action::Reload(Box::new(settings))
        }
        Kind::Move => Action::Move(if now.held {
            drift(rng, now, now.last)
        } else {
            spot(rng, now, scene)
        }),
        Kind::Hold => Action::Hold(path(rng, now, scene)),
        Kind::Release => Action::Release(drift(rng, now, now.last)),
        Kind::Drag => Action::Drag(path(rng, now, scene)),
        Kind::Click => Action::Click(spot(rng, now, scene)),
        Kind::DoubleClick => Action::DoubleClick(spot(rng, now, scene)),
        Kind::OtherMouse if rng.chance(0.5) => Action::RightClick(spot(rng, now, scene)),
        Kind::OtherMouse => Action::Wheel(spot(rng, now, scene), *rng.pick(&[120, -120, 360])),
        Kind::Escape => Action::Key(VK_ESCAPE, rng.chance(0.1)),
        Kind::Enter => Action::Key(VK_RETURN, rng.chance(0.1)),
        Kind::EditKey => Action::Key(*rng.pick(b"ZYC") as u16, true),
        Kind::OtherKey => {
            let key = *rng.pick(&[
                VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN, VK_TAB, VK_SPACE, VK_DELETE, 0x41, 0x58, 0x5A,
                0x43, 0x59,
            ]);
            // Ctrl+Z、Y、C は書き込みの操作なので、ここでは Ctrl なしで送る。
            Action::Key(key, rng.chance(0.3) && !b"ZYC".contains(&(key as u8)))
        }
        // 録画はカウントダウンでしか確かめられないので、ビデオを少し多めに選ぶ。ただし全画面の
        // ときは、ビデオが範囲を四角形に戻すので、全画面の切り取りに届くようカメラを多めに選ぶ。
        Kind::Camera if rng.chance(if now.mode == MODE_SCREEN { 0.3 } else { 0.6 }) => {
            Action::Toolbar(VIDEO_BUTTON)
        }
        Kind::Camera => Action::Toolbar(CAMERA_BUTTON),
        Kind::AreaMenu => {
            let choices = [Some(0), Some(1), Some(2), Some(3), None];
            Action::AreaMenu(choices[rng.weighted(&[2, 2, 3, 5, 1])])
        }
        Kind::Pen => Action::Toolbar(EDIT_BUTTON),
        Kind::Leave => Action::Toolbar(*rng.pick(&[HISTORY_BUTTON, SETTINGS_BUTTON])),
        Kind::Close => Action::Toolbar(CLOSE_BUTTON),
        // 動かす手を半分にする: 残したツールバーが動かした所に戻るかは、動かしてからでないと見えない。
        Kind::Handle => match rng.below(6) {
            0 => {
                let (from, to) = two_buttons(rng, 7);
                Action::ToolbarSlip(from, to)
            }
            1 => Action::ToolbarHover(rng.range(0..=6)),
            2 => Action::ToolbarLeave,
            _ => Action::MoveToolbar(toolbar_spot(rng, now)),
        },
        Kind::BarTool => Action::Bar(*rng.pick(&[
            markup::probe::PEN,
            markup::probe::HIGHLIGHTER,
            markup::probe::ERASER,
        ])),
        Kind::BarEdit => Action::Bar(*rng.pick(&[
            markup::probe::UNDO,
            markup::probe::REDO,
            markup::probe::CLEAR,
        ])),
        Kind::BarCapture => Action::Bar(markup::probe::CAPTURE),
        Kind::BarClose => Action::Bar(markup::probe::CLOSE),
        Kind::BarPointer => match rng.below(3) {
            0 => {
                let (from, to) = two_buttons(rng, 8);
                Action::BarSlip(from, to)
            }
            1 => Action::BarHover(rng.range(0..=7)),
            _ => Action::BarLeave,
        },
        Kind::ToastPointer => {
            let at = *rng.pick(&[Spot::Close, Spot::Button, Spot::Text, Spot::Menu]);
            match rng.below(5) {
                0 | 1 => Action::ToastHover(at),
                2 => Action::ToastLeave,
                _ if at == Spot::Menu => Action::ToastClick(Spot::Text),
                _ => Action::ToastClick(at),
            }
        }
        Kind::ToastMenu => Action::ToastMenu(*rng.pick(&[Some(0), Some(1), None])),
        Kind::ToastTimer if rng.chance(0.5) => Action::ToastDismiss,
        // 滑り終える前（0、125、200 ミリ秒）と、滑り終えた後（400 ミリ秒）の刻み。
        Kind::ToastTimer => Action::ToastSlide(*rng.pick(&[0, 125, 200, 400])),
        Kind::Tick if rng.chance(0.25) => Action::CountPaint,
        Kind::Tick => Action::Tick,
        Kind::CountClick => Action::CountClick,
        Kind::RecordStop => Action::RecordStop,
        // 赤い点（20 DIP）と経過時間（60 DIP）。
        Kind::RecordOther if rng.chance(0.4) => Action::RecordClick(*rng.pick(&[20.0, 60.0])),
        Kind::RecordOther => Action::RecordTick,
        Kind::System => Action::System(*rng.pick(&[
            Sys::DisplayChange,
            Sys::DpiChanged,
            Sys::SettingChange,
            Sys::CancelMode,
        ])),
        Kind::Wait => Action::Wait(rng.range(20..=250)),
    }
}
