//! ほかの PC や利用者が書いたかもしれない書きかけと、最後に書かれてからの時間。
//!
//! | 項目 | 技法 | ケース → 期待値 |
//! |---|---|---|
//! | 経ったか | 境界値（100 ns 刻み） | 最後に書かれてから 59 秒・60 秒ちょうど → 経っていない; 60 秒と 100 ns → 経った（`>`）; 未来の時刻、読めない → 経っていない |
//! | ほかの PC や利用者の書きかけ | 境界値（秒）× 決定表（持ち主 × 種類） | 書いたプロセスが終わり 4 分 55 秒前 → 残す; 5 分 5 秒前 → 消す（境目は README の 5 分。テストにかかる時間を見込んで 5 秒ずつ離す）; 動いているプロセス、書き終えた録画、未来の時刻 → 残す |
//! | この PC のこの利用者の書きかけ | 決定表 | 書いたばかりでも、書いたプロセスが終わっていれば消す; ファイルごとに見る |
//! | 場所 | 同値分割 | この PC のドライブ → この PC; 共有フォルダー（`\\localhost\C$`）→ ほか |
//! | 持ち主 | 同値分割 | このプロセスが作ったファイル → この利用者; Windows のファイル（TrustedInstaller）、無いファイル → ほか |

use super::*;

#[test]
fn a_file_is_untouched_only_after_more_than_the_time() {
    let sandbox = Sandbox::new();
    let path = sandbox.path().join("a.png.1.partial");
    fs::write(&path, b"x").unwrap();
    let written = fs::metadata(&path).unwrap().modified().unwrap();
    let minute = Duration::from_secs(60);
    // ファイルの時刻は 100 ns 刻み。境目ちょうどは、まだ経っていない。
    let tick = Duration::from_nanos(100);
    assert!(!untouched_for(
        &path,
        written + Duration::from_secs(59),
        minute
    ));
    assert!(!untouched_for(&path, written + minute, minute));
    assert!(untouched_for(&path, written + minute + tick, minute));
    // 時計が戻って未来の時刻になっているときと、時刻を読めないときは、経っていない。
    assert!(!untouched_for(&path, written - tick, Duration::ZERO));
    assert!(!untouched_for(
        &sandbox.path().join("missing"),
        written + 60 * minute,
        minute
    ));
}

/// `folder` に、最後に書かれたのが今から `age` 前（負なら未来）のファイル `name` を作る。
fn file_at(folder: &Path, name: &str, bytes: &[u8], age: i64) -> PathBuf {
    let path = folder.join(name);
    fs::write(&path, bytes).unwrap();
    let now = SystemTime::now();
    let seconds = Duration::from_secs(age.unsigned_abs());
    let time = if age >= 0 {
        now - seconds
    } else {
        now + seconds
    };
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(time)
        .unwrap();
    path
}

#[test]
fn a_leftover_that_may_be_written_elsewhere_is_removed_only_after_five_minutes() {
    let sandbox = Sandbox::new();
    let folder = sandbox.path();
    let finished = finished_process();
    let (dead, own) = (finished.id(), std::process::id());
    let moov = [&mp4_box(b"ftyp", b"isom")[..], &mp4_box(b"moov", &[0; 8])].concat();
    // 境目は 5 分（300 秒）。
    let gone = [
        file_at(folder, &format!("old.png.{dead}.partial"), b"x", 305),
        file_at(folder, &format!("old.mp4.{dead}.partial"), b"x", 305),
    ];
    let kept = [
        file_at(folder, &format!("new.png.{dead}.partial"), b"x", 295),
        file_at(folder, &format!("new.bmp.{dead}.partial"), b"x", -60),
        // 動いているプロセスと、書き終えた録画は、古くても残す。
        file_at(folder, &format!("mine.png.{own}.partial"), b"x", 3600),
        file_at(folder, &format!("done.mp4.{dead}.partial"), &moov, 3600),
    ];
    tidy(folder, SystemTime::now(), |_| false);
    for path in &gone {
        assert!(!path.exists(), "{path:?} is removed");
    }
    for path in &kept {
        assert!(path.exists(), "{path:?} stays");
    }
    // この PC のこの利用者が書いたとわかるものは、書いたばかりでも、書いたプロセスが終わっていれば
    // 消す。ファイルごとに見る。
    let fresh = file_at(folder, &format!("fresh.png.{dead}.partial"), b"x", 0);
    tidy(folder, SystemTime::now(), |file| {
        file == fresh || file == kept[0]
    });
    assert!(!fresh.exists() && !kept[0].exists());
    assert!(kept[1..].iter().all(|path| path.exists()));
}

#[test]
fn only_a_drive_of_this_pc_and_a_file_of_this_user_count_as_written_here() {
    let sandbox = Sandbox::new();
    assert!(origin::on_this_pc(sandbox.path()));
    // この PC の管理共有も、共有フォルダーとして開けば、ほかの PC と同じネットワークの場所。
    assert!(!origin::on_this_pc(Path::new(r"\\localhost\C$\Windows")));
    let mine = sandbox.path().join("mine.png");
    fs::write(&mine, b"x").unwrap();
    assert!(origin::owned_here(&mine));
    // Windows のファイルの持ち主は TrustedInstaller。
    let system = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32")
        .join("kernel32.dll");
    assert!(system.exists());
    assert!(!origin::owned_here(&system));
    assert!(!origin::owned_here(&sandbox.path().join("missing")));
}
