//! モンキーテスト: トレイ、オーバーレイ、ツールバー、書き込みのバー、通知、カウントダウンに、
//! 実際に届く操作を無作為な順で送り、1 手ごとに不変条件を確かめる。本物の状態とウィンドウを
//! `Desk` で動かし、テストがメッセージループとマウスとキーボードの役をする（本物のマウスと
//! キーボードには触れない）。
//!
//! 操作と重みは `actions.rs`、送り方は `drive.rs`、不変条件と操作ごとの結果は `oracle.rs`、
//! 届いた画像の数え方は `sinks.rs`、的のウィンドウ・設定と履歴の代わり・ダイアログの見張りは
//! `scene.rs` にある。
//!
//! 送らない操作、無害にしてから送る操作:
//! - 本物の録画: 設定のカウントダウンを常にオンで書き、出てきたカウントダウンの 1 秒のタイマーは
//!   すぐ止める。テストが送る刻みは残りが 2 以上のときだけなので、録画は始まらない。
//! - 名前を付けて保存のダイアログ: 「保存時にファイル名を指定する」を常にオフで書く。
//! - 別のプロセス: 設定と履歴は、隠れた代わりのウィンドウ（`stub_window`）を見つけて前に出すだけ
//!   になる。About、通知から開くファイル、プレイヤーは、テストではシェルに頼まず記録だけする
//!   （`imaging::shell_open` と `launch_player` の継ぎ目）。About のページは一時フォルダーに書くので、
//!   TMP と TEMP をサンドボックスに向ける。
//! - 終了（TRAY_EXIT）: プロセスごと終わり、続きの手がなくなる（既存のテストが確かめる）。
//! - ツールバーの本物のドラッグ: DefWindowProc の移動のループは本物のマウスを待つので、
//!   ヒットテストと、ループが送る WM_ENTERSIZEMOVE・位置の変更・WM_EXITSIZEMOVE を送る。
//! - ほかのアプリのウィンドウ: ウィンドウを選ぶ点は、テストが出した的の中だけにする。
//!
//! 長く回すとき: `$env:SNIPPING_LITE_CASES='400'`（シードは `SNIPPING_LITE_SEED`）。各ケースの
//! 始めに、そのケースだけを回す値を標準エラーへ直接書く。Win32 のコールバックの中の panic は
//! プロセスを止め、キャプチャした出力も `Rng::note` の手順も失われるため。

use super::*;
use crate::test_support::{
    desk::Desk,
    gui_objects,
    rng::{self, Reach, Rng},
};
use std::{
    io::Write as _,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

mod actions;
mod drive;
mod oracle;
mod scene;
mod sinks;
mod tally;

use actions::Action;
use scene::Scene;
use sinks::Sinks;
use tally::Tally;

const NAME: &str = "random_input_keeps_the_tray_and_its_windows_consistent";
/// 既定のケース数。ケースごとに新しいトレイ（`Desk`）と設定で始める。モード、画像と録画、
/// 書き込みのオン・オフはケースの中で持ち越すので、短いケースを多く回して偏りをならす。
const CASES: usize = 30;
/// 1 ケースの手数。
const STEPS: std::ops::RangeInclusive<usize> = 20..=50;
/// 待機に戻ったときに、最初の待機より増えてよいオブジェクトの数（書き込みのバーを初めて
/// 作る分、システムがあとから作るフォントやテーマの分）。
const SLACK: u32 = 6;

fn s() -> &'static mut State {
    // SAFETY: the test owns the state and runs on the thread that created it.
    unsafe { state() }
}

fn shown(hwnd: HWND) -> bool {
    // SAFETY: plain window query.
    !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) } != 0
}

fn window_rect(hwnd: HWND) -> RECT {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: plain window query.
    unsafe { GetWindowRect(hwnd, &mut rect) };
    rect
}

/// テストの側で覚えておく、利用者の操作から決まる値。
struct World {
    /// オーバーレイの上で左ボタンを押したまま。
    pub held: bool,
    /// 最後にオーバーレイへ送ったマウスの位置。
    pub last_point: actions::P,
    /// 範囲のメニュー、カメラとビデオのボタン、ペンのボタンだけが変える値。
    pub mode: i32,
    pub video: bool,
    pub markup: bool,
    /// 最後に確かめた届け先。
    pub last: Sinks,
    /// 届いた画像の数。
    pub delivered: usize,
    /// タイマーを止めたカウントダウンのウィンドウ。
    pub countdown: HWND,
    /// 最初の待機の GDI と USER のオブジェクトの数。
    pub objects: Option<(u32, u32)>,
    pub sandbox: PathBuf,
    pub shots: PathBuf,
}

/// 手を選ぶ前の様子。生成器と、操作ごとの結果の確かめに使う。
#[derive(Clone)]
struct Snapshot {
    pub active: bool,
    pub overlay: bool,
    /// 見えていれば、ツールバーの位置。
    pub toolbar: Option<RECT>,
    pub toolbar_rect: RECT,
    pub toolbar_open: bool,
    pub bar: bool,
    pub bar_rect: Option<RECT>,
    pub session: Option<markup::probe::Look>,
    pub toast: Option<toast::probe::Look>,
    /// カウントダウンの残り。
    pub count: Option<i32>,
    pub held: bool,
    pub last: actions::P,
    pub dragging: bool,
    pub start: POINT,
    pub end: POINT,
    pub points: usize,
    pub mode: i32,
    pub history: bool,
    pub width: i32,
    pub height: i32,
    pub origin: POINT,
}

impl Snapshot {
    fn take(world: &World) -> Snapshot {
        let st = s();
        let toolbar_rect = window_rect(st.toolbar);
        // SAFETY: the state belongs to this thread.
        let (session, toast) = unsafe { (markup::probe::look(), toast::probe::look()) };
        Snapshot {
            active: st.active,
            overlay: shown(st.overlay),
            toolbar: shown(st.toolbar).then_some(toolbar_rect),
            toolbar_rect,
            toolbar_open: st.toolbar_open,
            bar: shown(st.markup_bar),
            bar_rect: shown(st.markup_bar).then(|| window_rect(st.markup_bar)),
            session,
            toast,
            count: (!st.count_window.is_null()).then_some(st.count_left),
            held: world.held,
            last: world.last_point,
            dragging: st.dragging,
            start: st.start,
            end: st.end,
            points: st.points.len(),
            mode: st.mode,
            history: st.history,
            width: st.width,
            height: st.height,
            origin: st.origin,
        }
    }

    /// 失敗したときに表示する、短い様子。
    fn describe(&self) -> String {
        let state = if let Some(left) = self.count {
            format!("countdown {left}")
        } else if self.session.is_some() {
            "marking up".into()
        } else if self.active {
            format!("snipping mode {}", self.mode)
        } else {
            "idle".into()
        };
        let extra = [
            (self.held, " held"),
            (self.dragging, " dragging"),
            (self.toolbar.is_some(), " toolbar"),
            (self.toast.is_some(), " toast"),
        ];
        let extra: String = extra
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| *name)
            .collect();
        format!("[{state}{extra}]")
    }

    /// 手を選ぶ前の状態を数える。
    fn reach(&self, reach: &mut Reach) {
        let st = s();
        if !self.active && self.count.is_none() {
            reach.hit(if self.toolbar.is_some() {
                "idle with the kept toolbar"
            } else {
                "idle"
            });
        }
        if self.active && self.session.is_none() {
            reach.hit(match self.mode {
                MODE_FREE => "snipping freeform",
                MODE_WINDOW => "snipping a window",
                MODE_SCREEN => "snipping the full screen",
                _ => "snipping a rectangle",
            });
            if st.video {
                reach.hit("snipping a recording");
            }
            reach.hit(if st.screen_dc.is_null() {
                "live"
            } else {
                "frozen"
            });
        }
        let hits = [
            (self.dragging, "selecting"),
            (self.session.is_some(), "marking up"),
            (self.count.is_some(), "countdown"),
            (self.toast.is_some(), "notification"),
        ];
        for (on, label) in hits {
            if on {
                reach.hit(label);
            }
        }
    }
}

#[test]
#[ignore = "Covers the screen"]
fn random_input_keeps_the_tray_and_its_windows_consistent() {
    let mut tally = Tally::new();
    let (mut case, mut steps, mut images) = (0u64, 0, 0);
    let mut user_objects: Option<u32> = None;
    let started = Instant::now();
    let cases = rng::check(NAME, CASES, |rng| {
        let seed = rng::seed().wrapping_add(case);
        // Win32 のコールバックの中の panic は出力のキャプチャごと失われるので、直接書く。
        let _ = writeln!(
            std::io::stderr(),
            "{NAME}: case {case}: SNIPPING_LITE_SEED={seed:#x} SNIPPING_LITE_CASES=1"
        );
        case += 1;
        let (count, delivered) = run_case(rng, &mut tally);
        steps += count;
        images += delivered;
        // ケースを重ねても USER のオブジェクトは増え続けない。GDI は数えない: テストの `Desk` は
        // 起動のたびに窓のクラスを登録し直し、使われない枠のブラシを 1 つ作る（本物は 1 回だけ）。
        let user = gui_objects().1;
        let first = *user_objects.get_or_insert(user);
        assert!(
            user <= first + SLACK,
            "USER objects went from {first} to {user} over the cases"
        );
    });
    eprintln!(
        "{NAME}: {cases} cases, {steps} steps, {images} images delivered in {:?}",
        started.elapsed()
    );
    tally.finish(steps);
}

/// 1 ケース: 新しいトレイを無作為な設定で始め、手を送り、最後に待機へ戻す。
/// (手数, 届いた画像の数) を返す。
fn run_case(rng: &mut Rng, tally: &mut Tally) -> (usize, usize) {
    let mut desk = Desk::start_with(|settings| {
        scene::randomize(rng, settings);
        config::save_markup(rng.chance(0.6));
    });
    let temporary = desk.sandbox.path().join("temp");
    desk.sandbox.set("TMP", &temporary);
    desk.sandbox.set("TEMP", &temporary);
    let scene = Scene::new();
    let shots = desk.sandbox.path().join("shots");
    let st = s();
    let mut world = World {
        held: false,
        last_point: (0, 0),
        mode: st.mode,
        video: st.video,
        markup: st.markup,
        last: Sinks::observe(&shots),
        delivered: 0,
        countdown: null_mut(),
        objects: None,
        sandbox: desk.sandbox.path().to_path_buf(),
        shots,
    };
    crate::test_support::take_shell_opened();
    let count = rng.range(STEPS);
    for step in 0..count {
        let before = Snapshot::take(&world);
        let action = actions::next(rng, &before, &scene);
        rng.note(format!("{step} {} {action:?}", before.describe()));
        tally.step(&before, &action);
        let mut hands = drive::Hands {
            desk: &desk,
            world: &mut world,
            scene: &scene,
        };
        drive::apply(&mut hands, &action);
        oracle::after_step(&mut world, &scene, &before, &action);
    }
    let mut hands = drive::Hands {
        desk: &desk,
        world: &mut world,
        scene: &scene,
    };
    drive::settle(&mut hands);
    rng.note("settled");
    let before = Snapshot::take(&world);
    oracle::after_step(&mut world, &scene, &before, &Action::Wait(0));
    assert!(!s().active, "the case ends idle");
    let delivered = world.delivered;
    crate::ui::set_theme_choice(config::AppTheme::System);
    drop(scene);
    drop(desk);
    (count, delivered)
}
