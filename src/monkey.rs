//! モンキーテスト: トレイ、オーバーレイ、ツールバー、書き込みのバー、通知、カウントダウン、
//! 録画中の表示に、実際に届く操作を無作為な順で送り、1 手ごとに不変条件を確かめる。本物の状態と
//! ウィンドウを `Desk` で動かし、テストがメッセージループとマウスとキーボードと時計の役をする
//! （本物のマウスのボタンとキーボードには触れない）。
//!
//! 操作と重みは `actions.rs`、送り方は `drive.rs`、不変条件と操作ごとの結果は `oracle.rs`、
//! 届いた画像の数え方は `sinks.rs`、録画の確かめは `recording.rs`、的のウィンドウ・設定と履歴の
//! 代わり・ダイアログの見張りは `scene.rs` にある。
//!
//! 同じシードからは同じ手順になるように、手の速さや本物の時計と画面で決まるものを除く:
//! - 本物の時計で来るこのアプリのタイマー（通知を滑らせる・閉じる、ツールバーを上げ直す、
//!   カウントダウン、録画の表示）は流さず（`drive::real_timer`）、テストが WM_TIMER を送る。
//!   通知の滑りは、滑り始めてからたった時間をテストが決めてから送る（`toast::probe::age_slide`）。
//! - カーソルの位置（ツールバーを出すモニター、画面の倍率、窓の強調が決まる）は、ケースの初めに
//!   プライマリモニターの真ん中へ置き、テストの終わりに元へ戻す。
//!
//! 送らない操作、無害にしてから送る操作:
//! - 本物の録画: 設定のカウントダウンを常にオンで書く。カウントダウンの 1 秒のタイマーはすぐ止め、
//!   テストが刻みを送る。最後の刻みで本物の録画が始まる（音声は録らない設定にする）。
//! - 名前を付けて保存のダイアログ: 「保存時にファイル名を指定する」を常にオフで書く。
//! - 別のプロセス: 設定と履歴は、隠れた代わりのウィンドウ（`stub_window`）を見つけて前に出すだけ
//!   になる。About、通知から開くファイル、プレイヤーは、テストではシェルに頼まず記録だけする
//!   （`imaging::shell_open` と `launch_player` の継ぎ目）。About のページは一時フォルダーに書くので、
//!   TMP と TEMP をサンドボックスに向ける。
//! - 終了（TRAY_EXIT）: プロセスごと終わり、続きの手がなくなる（既存のテストが確かめる）。
//!   録画中にケースが終わるときだけ、最後に送って、終了が録画をファイルに仕上げることを確かめる。
//! - ツールバーの本物のドラッグ: DefWindowProc の移動のループは本物のマウスを待つので、
//!   ヒットテストと、ループが送る WM_ENTERSIZEMOVE・位置の変更・WM_EXITSIZEMOVE を送る。
//! - ほかのアプリのウィンドウ: ウィンドウを選ぶ点は、テストが出した的の中だけにする。
//!
//! 長く回すとき: `$env:SNIPPING_LITE_CASES='400'`（シードは `SNIPPING_LITE_SEED`）。各ケースの
//! 始めに、そのケースだけを回す値を標準エラーへ直接書く。Win32 のコールバックの中の panic は
//! プロセスを止め、キャプチャした出力も `Rng::note` の手順も失われるため。

use super::*;
use crate::test_support::{
    desk::{Desk, Turn},
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
mod recording;
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
    /// 録画中なら、カウントダウンの最後の刻みで決まったこと。
    pub recording: Option<recording::Started>,
    /// 仕上がった録画の数。
    pub recorded: usize,
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
    pub recording: bool,
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
            recording: st.recording.is_some(),
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
        } else if self.recording {
            "recording".into()
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

    /// 手を選ぶ前の状態の名前（`actions::STATES`）。
    fn states(&self) -> Vec<&'static str> {
        let st = s();
        let mut states = Vec::new();
        if self.recording {
            states.push("recording");
        } else if !self.active && self.count.is_none() {
            states.push(if self.toolbar.is_some() {
                "idle with the kept toolbar"
            } else {
                "idle"
            });
        }
        if self.active && self.session.is_none() {
            states.push(match self.mode {
                MODE_FREE => "snipping freeform",
                MODE_WINDOW => "snipping a window",
                MODE_SCREEN => "snipping the full screen",
                _ => "snipping a rectangle",
            });
            if st.video {
                states.push("snipping a recording");
            }
            states.push(if st.screen_dc.is_null() {
                "live"
            } else {
                "frozen"
            });
        }
        let flags = [
            (self.dragging, "selecting"),
            (self.session.is_some(), "marking up"),
            (self.count.is_some(), "countdown"),
            (self.toast.is_some(), "notification"),
        ];
        states.extend(flags.iter().filter(|(on, _)| *on).map(|&(_, label)| label));
        states
    }
}

#[test]
#[ignore = "Covers the screen"]
fn random_input_keeps_the_tray_and_its_windows_consistent() {
    let mut tally = Tally::new();
    let (mut case, mut steps, mut images) = (0u64, 0, 0);
    // どの状態と操作にも届いた後に始めたケースが残した GDI のオブジェクトの合計と、そのケースの数。
    let (mut gdi, mut counted) = (0i64, 0);
    // 最初のケースを片付けた後の USER のオブジェクトの数。
    let mut user_after: Option<u32> = None;
    // ケースごとに Desk を作り直すが、番は初めに 1 度だけ取って持ち続ける。並べて走るほかの
    // テストがケースの間に割り込まず、その錠を待つ時間がケースの時間（`rng::CASE_LIMIT`）に入らない。
    let mut turn = Some(Turn::take());
    let cursor = Cursor::pin();
    // 仕上がった録画の数と、いちばん長くかかったケースの時間（上限は `rng::CASE_LIMIT`）。
    let (mut recordings, mut slowest) = (0, Duration::ZERO);
    let started = Instant::now();
    let cases = rng::check(NAME, CASES, |rng| {
        let seed = rng::seed().wrapping_add(case);
        // Win32 のコールバックの中の panic は出力のキャプチャごと失われるので、直接書く。
        let _ = writeln!(
            std::io::stderr(),
            "{NAME}: case {case}: SNIPPING_LITE_SEED={seed:#x} SNIPPING_LITE_CASES=1"
        );
        case += 1;
        let warmed = tally.warmed();
        cursor.center();
        let begun = Instant::now();
        let (count, delivered, recorded, left) = run_case(rng, &mut tally, &mut turn);
        slowest = slowest.max(begun.elapsed());
        steps += count;
        images += delivered;
        // プロセスで初めての録画は、1 度だけ作るオブジェクトを残す（recording.rs）。片付けた後の
        // USER のオブジェクトは、そのケースから数え直す。
        if recordings == 0 && recorded > 0 {
            user_after = None;
        }
        recordings += recorded;
        // ケースを重ねても、オブジェクトは増え続けない。1 ケースの撮影は数回なので、1 回に 1 つの
        // 漏れは 1 ケースでは見えにくい。ケースが残した GDI のオブジェクトを足して、全体で上限を
        // 見る。プロセスで 1 度だけ作るものがそろうまで（どの状態と操作にも届くまで）は数えない。
        if warmed {
            gdi += left;
            counted += 1;
        }
        assert!(
            gdi <= SLACK as i64,
            "the {counted} cases since every state and kind was reached left {gdi} GDI objects"
        );
        // USER のオブジェクト（ケースの間に作るスレッドの窓など）は片付けで消えるので、片付けた
        // 後の数で比べる。
        let user = gui_objects().1;
        let first = *user_after.get_or_insert(user);
        assert!(
            user <= first + SLACK,
            "USER objects after a case went from {first} to {user}"
        );
    });
    drop(cursor);
    eprintln!(
        "{NAME}: {cases} cases, {steps} steps, {images} images delivered, {recordings} recordings \
         finished in {:?} (the slowest case {slowest:?}); the last {counted} cases left {gdi} GDI \
         objects",
        started.elapsed()
    );
    tally.finish(steps);
}

/// カーソルの位置。ツールバーを出すモニター、画面の倍率、撮影を始めたときの窓の強調はカーソルの
/// 位置で決まるので、ケースの初めに決まった所（プライマリモニターの真ん中）へ置く。テストの前の
/// 位置を覚えておき、終わりに戻す。
struct Cursor(POINT);

impl Cursor {
    fn pin() -> Cursor {
        let mut was = POINT { x: 0, y: 0 };
        // SAFETY: a plain cursor query.
        unsafe { GetCursorPos(&mut was) };
        Cursor(was)
    }

    fn center(&self) {
        // SAFETY: plain metric queries and a cursor move.
        unsafe {
            SetCursorPos(
                GetSystemMetrics(SM_CXSCREEN) / 2,
                GetSystemMetrics(SM_CYSCREEN) / 2,
            )
        };
    }
}

impl Drop for Cursor {
    fn drop(&mut self) {
        // SAFETY: a plain cursor move.
        unsafe { SetCursorPos(self.0.x, self.0.y) };
    }
}

/// 1 ケース: 新しいトレイを無作為な設定で始め、手を送り、最後に待機へ戻す。録画中に終わるときは、
/// 終了させて録画が仕上がることを確かめる。
/// (手数, 届いた画像の数, 仕上がった録画の数, 始めから終わりまでに増えた GDI のオブジェクト) を返す。
/// 始めはトレイと舞台を作る前、終わりは片付けた後に数える。トレイの起動と片付けが返さない
/// もの（起動のたびの窓のクラスの登録など）も、ケースが残したものに入る。
fn run_case(
    rng: &mut Rng,
    tally: &mut Tally,
    turn: &mut Option<Turn>,
) -> (usize, usize, usize, i64) {
    let first = gui_objects();
    let held = turn.take().expect("the turn comes back after each case");
    let mut desk = Desk::start_in(held, |settings| {
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
        recording: None,
        recorded: 0,
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
    let before = Snapshot::take(&world);
    oracle::after_step(&mut world, &scene, &before, &Action::Wait(0));
    assert!(!s().active, "the case ends idle");
    if s().recording.is_some() {
        // 終了は、プロセスごと終わる前に録画をファイルに仕上げる（docs/behavior.md）。終わったトレイには
        // もう手を送らない。
        let mut hands = drive::Hands {
            desk: &desk,
            world: &mut world,
            scene: &scene,
        };
        drive::exit(&mut hands);
    }
    rng.note(format!(
        "settled: {} images, {} recordings",
        world.delivered, world.recorded
    ));
    crate::ui::set_theme_choice(config::AppTheme::System);
    drop(scene);
    *turn = Some(desk.into_turn());
    let left = gui_objects().0 as i64 - first.0 as i64;
    (count, world.delivered, world.recorded, left)
}
