//! 一時名、名前の付け替え、残った一時名の掃除。
//!
//! | 項目 | 技法 | ケース → 期待値 |
//! |---|---|---|
//! | 一時名 | 例 | `a.png` → `a.png.<このプロセスの ID>.partial` |
//! | 掃除の対象 | 同値分割 | png、bmp、mp4（大文字も）の書きかけ → 書いたプロセスの ID; ほかの種類、ID のない名前、数字でない ID、最後の名前 → 対象外 |
//! | 持ち主 | 状態 | このプロセス → 動いている; 終わったプロセス（ハンドルを持ち、ID が使い回されない）→ 動いていない |
//! | 掃除 | 決定表（持ち主 × 種類） | 終わったプロセスの書きかけ → 消す; 動いているプロセスの書きかけ、利用者のファイル → 残す |
//! | 書き終えた録画 | 決定表（種類 × MP4 の最上位のボックス × 読めるか） | 終わったプロセスの一時名の MP4 で、ファイルに収まる `moov` がある（大きさ 64 ビットの `mdat` の後も）→ 残す; 読めない → 残す; `moov` がない、`mdat` が終わりまで・ファイルより大きい、`moov` が途中で切れている、`mdat` の中のデータに `moov` の文字がある、見出しより小さい大きさ、64 ビットの大きさが切れている、空 → 消す; 同じ中身の PNG の一時名 → 消す |
//! | 付け替え | 状態 | 前のファイルがある → 置き換える; 名前を変えられない → 一時名を消して失敗を返す |
//! | 残す（録画） | 状態 | 名前を変えられる → 最後の名前; 最後の名前が開かれている → 空いた `(n)`（埋まった番号は置き換えない）; 一時名が開かれ読める → `(n)` へ写す; 読めもしない → 一時名を残して失敗 |
//! | 別名 | 例 | `a.mp4` → `a (2).mp4`; 点を含む名前、拡張子なし |

use super::*;
use crate::test_support::Sandbox;

/// 終わったプロセス。ハンドルを持っている間、その ID はほかのプロセスに使い回されない。
struct Finished(std::process::Child);

impl Finished {
    fn id(&self) -> u32 {
        self.0.id()
    }
}

fn finished_process() -> Finished {
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "exit 0"])
        .spawn()
        .expect("cmd starts");
    child.wait().unwrap();
    Finished(child)
}

#[test]
fn a_file_is_written_under_a_name_that_carries_the_writer() {
    let path = Path::new("D:\\Shots\\Snipping Lite 1.png");
    assert_eq!(
        partial(path),
        PathBuf::from(format!(
            "D:\\Shots\\Snipping Lite 1.png.{}.partial",
            std::process::id()
        ))
    );
    assert_eq!(
        partial(path)
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(writer_of),
        Some(("png", std::process::id()))
    );
}

#[test]
fn only_our_pictures_and_recordings_count_as_leftovers() {
    let cases = [
        ("Snipping Lite 1.png.42.partial", Some(("png", 42))),
        ("1793151938315.thumb.png.7.partial", Some(("png", 7))),
        (
            "Snipping Lite 2.bmp.4294967295.partial",
            Some(("bmp", u32::MAX)),
        ),
        ("Snipping Lite 3.mp4.9.partial", Some(("mp4", 9))),
        ("SHOUTED.PNG.5.partial", Some(("png", 5))),
        ("SHOUTED.MP4.6.partial", Some(("mp4", 6))),
        // 対象外: ほかの種類、書いたプロセスのない名前、数字でない ID、桁あふれ、最後の名前。
        ("notes.txt.5.partial", None),
        ("archive.5.partial", None),
        ("a.png.partial", None),
        ("a.png..partial", None),
        ("a.png.12x.partial", None),
        ("a.png.-1.partial", None),
        ("a.png.4294967296.partial", None),
        ("a.png", None),
        ("a.png.5.partial.bak", None),
    ];
    for (name, writer) in cases {
        assert_eq!(writer_of(name), writer, "{name}");
    }
}

#[test]
fn a_finished_process_is_not_alive_and_this_one_is() {
    assert!(alive(std::process::id()));
    let finished = finished_process();
    assert!(!alive(finished.id()));
}

#[test]
fn leftovers_of_finished_processes_are_removed_and_the_rest_stays() {
    let sandbox = Sandbox::new();
    let folder = sandbox.path();
    let finished = finished_process();
    let file = |name: String| {
        let path = folder.join(name);
        fs::write(&path, b"x").unwrap();
        path
    };
    let gone = [
        file(format!("Snipping Lite 1.png.{}.partial", finished.id())),
        file(format!("Snipping Lite 2.mp4.{}.partial", finished.id())),
    ];
    let kept = [
        // このプロセスが書いている途中。
        file(format!(
            "Snipping Lite 3.png.{}.partial",
            std::process::id()
        )),
        // 利用者のファイルと、書き終えたファイル。
        file(format!("notes.txt.{}.partial", finished.id())),
        file("Snipping Lite 4.png".to_owned()),
    ];
    remove_leftovers(folder);
    for path in gone {
        assert!(!path.exists(), "{path:?}");
    }
    for path in kept {
        assert!(path.exists(), "{path:?}");
    }
    // フォルダーがなくても何もしない。
    remove_leftovers(&folder.join("missing"));
}

/// MP4 の最上位のボックス: 4 バイトの大きさ（見出しを含む）、4 バイトの種類、中身。
fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = u32::try_from(8 + body.len()).unwrap();
    [&size.to_be_bytes()[..], kind, body].concat()
}

/// `keep` がどの名前にも置けずに一時名のまま残した録画は、書き終えていて作り直せないので、
/// 書いたプロセスが終わった後の掃除（次の録画の開始）でも消えない。途中で止まった録画は消える。
#[test]
fn a_finished_recording_left_under_its_temporary_name_is_not_removed_as_a_leftover() {
    let sandbox = Sandbox::new();
    let folder = sandbox.path();
    let finished = finished_process();
    let ftyp = mp4_box(b"ftyp", b"isom\0\0\x02\0");
    let mdat = mp4_box(b"mdat", b"frames");
    let moov = mp4_box(b"moov", &mp4_box(b"mvhd", &[0; 12]));
    let video = [&ftyp[..], &mdat, &moov].concat();
    // 大きさ 1 の後に 64 ビットの大きさ（見出し 16 バイト + 中身 6 バイト）。
    let large_mdat = [&[0, 0, 0, 1][..], b"mdat", &22u64.to_be_bytes(), b"frames"].concat();
    // 大きさ 0 はファイルの終わりまで（書いている途中の mdat）。
    let open_mdat = [&ftyp[..], &[0, 0, 0, 0], b"mdat", b"frames"].concat();
    let cases: [(&str, Vec<u8>, bool); 11] = [
        ("finished", video.clone(), true),
        ("large mdat", [&ftyp[..], &large_mdat, &moov].concat(), true),
        ("no moov", [&ftyp[..], &mdat].concat(), false),
        ("mdat to the end", open_mdat, false),
        (
            "mdat larger than the file",
            [&ftyp[..], &[0, 0, 0x10, 0], b"mdat", b"frames"].concat(),
            false,
        ),
        ("moov cut short", video[..video.len() - 1].to_vec(), false),
        (
            "moov only inside the data",
            [&ftyp[..], &mp4_box(b"mdat", &moov)].concat(),
            false,
        ),
        // 見出しより小さい大きさ 4 を飛ばすと、続く 8 バイトが収まる moov に読める。
        (
            "size below the header",
            [&[0, 0, 0, 4][..], &[0, 0, 0, 8], b"moov"].concat(),
            false,
        ),
        (
            "64-bit size cut short",
            [&ftyp[..], &[0, 0, 0, 1], b"mdat", &[0, 0]].concat(),
            false,
        ),
        ("empty", Vec::new(), false),
        ("unreadable", video.clone(), true),
    ];
    let paths: Vec<PathBuf> = cases
        .iter()
        .enumerate()
        .map(|(n, (_, bytes, _))| {
            let path = folder.join(format!("Snipping Lite {n}.mp4.{}.partial", finished.id()));
            fs::write(&path, bytes).unwrap();
            path
        })
        .collect();
    // 書き終えた録画と同じ中身でも、画像の一時名は書きかけの画像として消える。
    let picture = folder.join(format!("Snipping Lite.png.{}.partial", finished.id()));
    fs::write(&picture, &video).unwrap();
    // 読むのは許さず、消すのだけ許して開いておく。中身を確かめられない。
    let unreadable = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(4) // FILE_SHARE_DELETE
            .open(paths.last().unwrap())
            .unwrap()
    };
    remove_leftovers(folder);
    drop(unreadable);
    for ((what, bytes, kept), path) in cases.iter().zip(&paths) {
        if *kept {
            assert_eq!(
                fs::read(path).ok().as_ref(),
                Some(bytes),
                "{what} stays as it was"
            );
        } else {
            assert!(!path.exists(), "{what} is removed");
        }
    }
    assert!(!picture.exists(), "a picture is never a finished recording");
}

#[test]
fn committing_replaces_the_file_or_removes_the_partial_one() {
    let sandbox = Sandbox::new();
    let path = sandbox.path().join("a.png");
    fs::write(&path, b"old").unwrap();
    let written = partial(&path);
    fs::write(&written, b"new").unwrap();
    commit(&written, &path).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"new");
    assert!(!written.exists());
    // フォルダーのある場所へは名前を変えられない。一時名は残さない。
    let blocked = sandbox.path().join("b.png");
    fs::create_dir(&blocked).unwrap();
    let written = partial(&blocked);
    fs::write(&written, b"new").unwrap();
    assert!(commit(&written, &blocked).is_err());
    assert!(!written.exists() && blocked.is_dir());
}

/// `path` を共有なしで開いたままにする。開いている間は、その名前への名前の変更も削除もできない。
fn hold(path: &Path) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap()
}

#[test]
fn keeping_uses_the_final_name_when_it_can() {
    let sandbox = Sandbox::new();
    let path = sandbox.path().join("a.mp4");
    let written = partial(&path);
    fs::write(&written, b"video").unwrap();
    assert_eq!(keep(&written, &path).unwrap(), path);
    assert_eq!(fs::read(&path).unwrap(), b"video");
    assert!(!written.exists());
}

#[test]
fn a_held_final_name_keeps_the_file_under_the_next_free_number() {
    let sandbox = Sandbox::new();
    let path = sandbox.path().join("Snipping Lite 1.mp4");
    fs::write(&path, b"held").unwrap();
    // (2) は利用者のファイルで埋まっている。置き換えずに (3) へ。
    let taken = sandbox.path().join("Snipping Lite 1 (2).mp4");
    fs::write(&taken, b"mine").unwrap();
    let written = partial(&path);
    fs::write(&written, b"video").unwrap();
    let held = hold(&path);
    let kept = keep(&written, &path).unwrap();
    drop(held);
    assert_eq!(kept, sandbox.path().join("Snipping Lite 1 (3).mp4"));
    assert_eq!(fs::read(&kept).unwrap(), b"video");
    assert_eq!(fs::read(&path).unwrap(), b"held");
    assert_eq!(fs::read(&taken).unwrap(), b"mine");
    assert!(!written.exists());
    // 残した録画は、終わったプロセスの書きかけを消す掃除の対象にならない。
    assert_eq!(writer_of("Snipping Lite 1 (3).mp4"), None);
    remove_leftovers(sandbox.path());
    assert!(kept.exists());
}

#[test]
fn a_partial_file_that_cannot_be_renamed_is_copied_and_never_removed_on_failure() {
    let sandbox = Sandbox::new();
    let path = sandbox.path().join("b.mp4");
    let written = partial(&path);
    fs::write(&written, b"video").unwrap();
    // 一時名のファイル自体を、読むのだけ許して開いておく。名前は変えられないが写せる。
    let reader = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(1) // FILE_SHARE_READ
            .open(&written)
            .unwrap()
    };
    let kept = keep(&written, &path).unwrap();
    drop(reader);
    assert_eq!(kept, sandbox.path().join("b (2).mp4"));
    assert_eq!(fs::read(&kept).unwrap(), b"video");
    // 写すのもできないときは、一時名のファイルを消さずに失敗を返す。
    let path = sandbox.path().join("c.mp4");
    let written = partial(&path);
    fs::write(&written, b"video").unwrap();
    let held = hold(&written);
    assert!(keep(&written, &path).is_err());
    drop(held);
    assert_eq!(fs::read(&written).unwrap(), b"video");
    assert!(!path.exists() && !sandbox.path().join("c (2).mp4").exists());
}

#[test]
fn the_other_names_follow_the_numbering_of_explorer() {
    let cases = [
        (
            "D:\\v\\Snipping Lite 1.mp4",
            2,
            "D:\\v\\Snipping Lite 1 (2).mp4",
        ),
        ("D:\\v\\a.b.mp4", 20, "D:\\v\\a.b (20).mp4"),
        ("D:\\v\\plain", 3, "D:\\v\\plain (3)"),
    ];
    for (path, n, other) in cases {
        assert_eq!(
            alternate(Path::new(path), n),
            PathBuf::from(other),
            "{path}"
        );
    }
}
