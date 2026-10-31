//! テストのバイナリーを子プロセスとして起動し、子のテスト関数（`child_` で始まり、単独では何も
//! しない）を 1 つだけ走らせる仕組み。親は子に値を 1 つ渡せる（`SNIPPING_LITE_CHILD`）。子が
//! 終わらなくても、親が先に失敗しても、子は殺して待つ。子の出力はサンドボックスの中のファイルに書く。

use super::Sandbox;
use std::{
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
    time::Duration,
};

const VALUE: &str = "SNIPPING_LITE_CHILD";
/// 子が終わったかを確かめる間隔。
const POLL: Duration = Duration::from_millis(10);

/// 子プロセスとして起動されたなら、親が渡した値。子のテスト関数は、なければすぐ戻る。
pub fn value() -> Option<String> {
    std::env::var(VALUE).ok()
}

pub struct Child {
    process: std::process::Child,
    log: PathBuf,
}

impl Child {
    /// `test`（`crash::test_name` で作る名前）を、`value` を渡した子プロセスで走らせる。
    pub fn start(sandbox: &Sandbox, test: &str, value: &str) -> Child {
        let log = sandbox.path().join("child.log");
        let output = std::fs::File::create(&log).unwrap();
        let process = Command::new(std::env::current_exe().unwrap())
            .args([
                test,
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(VALUE, value)
            .stdin(Stdio::null())
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .spawn()
            .expect("the test binary starts again as a child");
        Child { process, log }
    }

    /// 子が終わるのを、長くても `limit` だけ待つ。終わらなければ `None`。
    pub fn wait(&mut self, limit: Duration) -> Option<ExitStatus> {
        let polls = (limit.as_millis() / POLL.as_millis()).max(1);
        for _ in 0..polls {
            if let Some(status) = self.process.try_wait().unwrap() {
                return Some(status);
            }
            std::thread::sleep(POLL);
        }
        None
    }

    /// 子がここまでに書いた出力。
    pub fn said(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    pub fn id(&self) -> u32 {
        self.process.id()
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
