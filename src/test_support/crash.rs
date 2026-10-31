//! クラッシュ時の整合性のテストの仕組み。
//!
//! 親のテストは、このテストのバイナリーを子プロセスとして起動し、子のテスト関数（`child_` で始まり、
//! 単独では何もしない）に書き込みをさせる。製品のコードは段階を終えるたびに `staging::step` を呼ぶ。
//! 子は `SNIPPING_LITE_CRASH_AT` が k のとき、k 回目の段階でその名前を印のファイルに書いて待ち、
//! 親が `TerminateProcess` する。電源が落ちたときと同じく、子の後片付けは何も走らない。
//! 親は同じフォルダーで次の起動の処理を走らせ、確かめる。k は 1 から増やし、子が k 回目より前に
//! 最後まで終えたら止める（`sweep`）。`pause` は止まった子を殺さずに待たせ、同時に動いていて
//! 書いている途中の、もう 1 つのプロセスの役にする。
//!
//! 子の環境は親のサンドボックス（`APPDATA`、`LOCALAPPDATA`、`USERPROFILE`）を受け継ぎ、一時フォルダー
//! （`TEMP`、`TMP`）もサンドボックスの中にする。子が失敗しても親が先に失敗しても、子は殺して待ち、
//! サンドボックスはその後で消える。

use super::Sandbox;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

const AT: &str = "SNIPPING_LITE_CRASH_AT";
const MARKER: &str = "SNIPPING_LITE_CRASH_MARKER";
/// 子を待つ間隔。
const POLL: Duration = Duration::from_millis(10);
/// 子が段階に着くか終わるまで待つ回数（60 秒）。テストのバイナリーの起動と GDI+ の初期化を含む。
const POLLS: usize = 6000;
/// 止まった子が、殺されないまま待つ回数（120 秒）。過ぎたら自分で終わり、それより先へは進まない。
const WAITS: usize = 2400;

static STEPS: AtomicUsize = AtomicUsize::new(0);

/// 子プロセスとして起動されたか。子のテスト関数は、そうでなければすぐ戻る。
pub fn is_child() -> bool {
    std::env::var_os(AT).is_some()
}

/// `staging::step` の中身。子プロセスの k 回目の段階なら、印を書いて殺されるのを待つ。
pub fn step(label: &str, path: &Path) {
    let Some(at) = std::env::var(AT).ok().and_then(|v| v.parse::<usize>().ok()) else {
        return;
    };
    if STEPS.fetch_add(1, Ordering::SeqCst) + 1 != at {
        return;
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    if let Some(marker) = std::env::var_os(MARKER).map(PathBuf::from) {
        // 親が書きかけの印を読まないよう、書いてから名前を変える。
        let writing = marker.with_extension("writing");
        if std::fs::write(&writing, format!("{label}: {name}")).is_ok() {
            let _ = std::fs::rename(&writing, &marker);
        }
    }
    for _ in 0..WAITS {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::process::exit(3);
}

/// `module_path!()` と関数の名前から、テストのバイナリーに渡すテストの名前を作る。
pub fn test_name(module: &str, function: &str) -> String {
    let module = module.split_once("::").map_or("", |(_, rest)| rest);
    format!("{module}::{function}")
}

/// 子プロセス。落とすときに殺して待つので、テストが失敗しても残らない。持っている間は、
/// 子のプロセス ID が別のプロセスに使い回されない（Windows はハンドルが閉じるまで ID を空けない）。
pub struct Crash {
    child: Child,
    /// 何回目の段階で止めるよう頼んだか（1 から）。
    pub at: usize,
    /// 止まった段階の名前。`None` は、子がその段階より前に最後まで終えたこと。
    pub stopped: Option<String>,
}

impl Crash {
    /// 子のプロセス ID。一時名に入る ID と比べる。
    pub fn id(&self) -> u32 {
        self.child.id()
    }

    /// 止まっている子を殺し、終わるまで待つ（`pause` の後で使う）。
    pub fn kill(&mut self) {
        self.child.kill().expect("the child is killed");
        self.child.wait().unwrap();
    }
}

impl Drop for Crash {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// `test` を子プロセスで走らせ、k 回目の段階で殺す。
pub fn run(sandbox: &Sandbox, test: &str, k: usize) -> Crash {
    let mut crash = pause(sandbox, test, k);
    if crash.stopped.is_some() {
        crash.kill();
    }
    crash
}

/// `test` を子プロセスで走らせ、k 回目の段階で止まるまで待つ。止まった子は殺さず、動いたまま
/// 待たせておく（書いている途中の、もう 1 つの Snipping Lite のプロセスの役）。`Crash::kill` か、
/// `Crash` を落とすときに殺す。止まる前に最後まで終えたら、`stopped` は `None`。
pub fn pause(sandbox: &Sandbox, test: &str, k: usize) -> Crash {
    let marker = sandbox.path().join(format!("crash-{k}.marker"));
    let log = sandbox.path().join(format!("crash-{k}.log"));
    let temp = sandbox.path().join("temp");
    std::fs::create_dir_all(&temp).unwrap();
    let output = std::fs::File::create(&log).unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            test,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(AT, k.to_string())
        .env(MARKER, &marker)
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .stdin(Stdio::null())
        .stdout(output.try_clone().unwrap())
        .stderr(output)
        .spawn()
        .expect("the test binary starts again as a child");
    let mut crash = Crash {
        child,
        at: k,
        stopped: None,
    };
    let said = || std::fs::read_to_string(&log).unwrap_or_default();
    for _ in 0..POLLS {
        if let Ok(label) = std::fs::read_to_string(&marker) {
            crash.stopped = Some(label);
            return crash;
        }
        if let Some(status) = crash.child.try_wait().unwrap() {
            assert!(
                status.success(),
                "{test} failed before step {k}:\n{}",
                said()
            );
            assert!(
                said().contains("1 passed"),
                "{test} did not run:\n{}",
                said()
            );
            return crash;
        }
        std::thread::sleep(POLL);
    }
    panic!("{test} neither reached step {k} nor finished:\n{}", said());
}

/// k = 1, 2, ... ごとに新しいサンドボックスを作り、`prepare` で同じ初めの状態にしてから `test` を
/// k 回目の段階で止め、`check` で確かめる。子が最後まで終えた回も `check` に渡して止める。
/// 止めた段階の名前を順に返す。`check` には子の `Crash`（頼んだ段階と止まった段階）を渡す。
/// `check` の間は子のハンドルを持っているので、子のプロセス ID はほかのプロセスに使い回されない。
pub fn sweep(
    test: &str,
    most: usize,
    prepare: impl Fn(&Sandbox),
    check: impl Fn(&Sandbox, &Crash),
) -> Vec<String> {
    let mut stages = Vec::new();
    for k in 1..=most {
        let sandbox = Sandbox::new();
        prepare(&sandbox);
        let crash = run(&sandbox, test, k);
        check(&sandbox, &crash);
        match crash.stopped.clone() {
            Some(label) => stages.push(label),
            None => return stages,
        }
    }
    panic!("{test} did not finish within {most} steps; it stopped at {stages:?}");
}
