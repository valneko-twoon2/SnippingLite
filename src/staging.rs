//! 段階を踏む書き込み（設定ファイル、画像、履歴、録画、一時フォルダーの掃除）の共通部品。
//!
//! 利用者のフォルダーへ書く画像と録画は、一時名 `<最後の名前>.<プロセス ID>.partial` で書いてから
//! 最後の名前に変える。途中でプロセスが止まっても、最後の名前で残るのは書き終えたファイルだけになる。
//! 残った一時名のファイルは、次に同じフォルダーへ書くときに消す（`remove_leftovers`）。画像は
//! `imaging::encode` が書く前にそのフォルダーを片付けるので、名前を付けて保存で選んだフォルダーも
//! 対象になる。録画は保存先と一時フォルダーを用意するとき、履歴は `history` の `tidy` が片付ける。
//! 消すのは、書いていたプロセスがもう動いていないものだけなので、別のプロセス（開いている履歴の
//! ウィンドウ）が書いている途中のファイルには触れない。書き終えた録画が最後の名前に置けないときは、
//! `<名前> (2).mp4` などの一時名でない名前に残す（`keep`）ので、この掃除の対象にならない。どの名前にも置けず一時名のまま
//! 残った録画も、書き終えている（MP4 の目次の `moov` ボックスがある）ので、掃除は消さない。
//!
//! 書き込みは段階を 1 つ終えるたびに `step` を呼ぶ。製品のビルドでは何もしない。テストのビルドでは、
//! クラッシュ時の整合性のテストが子プロセスをその段階で止めて殺し、残ったファイルを確かめる
//! （`test_support::crash`）。

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};
use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HANDLE};

/// 名前を変えるのを試す回数。書いたばかりのファイルを、ウイルス対策ソフトなどが少しの間開いていることがある。
const ATTEMPTS: usize = 5;
const RETRY: Duration = Duration::from_millis(20);
/// 一時名で書いてよい種類。掃除が利用者のほかのファイルに触れないよう、これ以外は消さない。
const KINDS: [&str; 3] = ["png", "bmp", "mp4"];

/// `path` を書いている途中の名前。最後の名前（`.png` など）では終わらないので、エクスプローラーや
/// 履歴の一覧に画像や動画として並ばない。
pub fn partial(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{}.partial", std::process::id()));
    PathBuf::from(name)
}

/// 書き終えた `partial` を `path` に変える。`path` があれば置き換える。できなければ `partial` を消す。
/// 画像と履歴はこちらを使う（画素はまだ手元やクリップボード、履歴にあり、履歴は決まった名前で探す）。
/// 作り直せない録画は `keep` を使う。
pub fn commit(partial: &Path, path: &Path) -> io::Result<()> {
    let result = rename_retrying(partial, path);
    if result.is_err() {
        let _ = fs::remove_file(partial);
    }
    result
}

/// 作り直せないファイル（書き終えた録画）を置く。`commit` と違い、`partial` を消さない。
///
/// 1. `path` に名前を変える（`commit` と同じく何度か試す）。
/// 2. できなければ、同じフォルダーの空いている `<名前> (2).<拡張子>`、`(3)` … に名前を変える
///    （エクスプローラーが重なった名前に付けるのと同じ連番。既にあるファイルは置き換えない）。
/// 3. それもできなければ（`partial` 自体が開かれているなど）、その連番の名前へ中身を写し、
///    写せたら `partial` を消す。
///
/// 返すのは置いた場所。どれもできなければ `partial` をそのまま残して失敗を返す。残した `partial` は
/// 書き終えた録画なので、書いたプロセスが終わった後の `remove_leftovers` も消さない。
pub fn keep(partial: &Path, path: &Path) -> io::Result<PathBuf> {
    let Err(error) = rename_retrying(partial, path) else {
        return Ok(path.to_owned());
    };
    for n in 2..=ALTERNATES {
        let other = alternate(path, n);
        if rename_new(partial, &other).is_ok() {
            return Ok(other);
        }
    }
    for n in 2..=ALTERNATES {
        let other = alternate(path, n);
        match copy_new(partial, &other) {
            Ok(()) => {
                let _ = fs::remove_file(partial);
                return Ok(other);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => break,
        }
    }
    Err(error)
}

/// 最後の名前が使えないときに試す連番の上限。
const ALTERNATES: u32 = 20;

/// `a.mp4` の `n` 番目の別名 `a (n).mp4`。
fn alternate(path: &Path, n: u32) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let name = match path.extension() {
        Some(extension) => format!("{stem} ({n}).{}", extension.to_string_lossy()),
        None => format!("{stem} ({n})"),
    };
    path.with_file_name(name)
}

fn rename_retrying(partial: &Path, path: &Path) -> io::Result<()> {
    let mut result = fs::rename(partial, path);
    for _ in 1..ATTEMPTS {
        if result.is_ok() {
            break;
        }
        std::thread::sleep(RETRY);
        result = fs::rename(partial, path);
    }
    result
}

/// `to` が無いときだけ名前を変える（`fs::rename` は既にあるファイルを置き換えてしまう）。
fn rename_new(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> BOOL;
    }
    let wide = |p: &Path| -> Vec<u16> { p.as_os_str().encode_wide().chain(Some(0)).collect() };
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both names are nul-terminated UTF-16 that outlive the call; flags 0 never replaces.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) } != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// `to` を新しく作って `from` の中身を写す。写しかけのファイルは残さない。
fn copy_new(from: &Path, to: &Path) -> io::Result<()> {
    let mut source = fs::File::open(from)?;
    let mut target = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    let copied = io::copy(&mut source, &mut target).and_then(|_| target.sync_all());
    if copied.is_err() {
        drop(target);
        let _ = fs::remove_file(to);
    }
    copied
}

/// `folder` に、もう動いていないプロセスが残した書きかけの画像と録画があれば消す。
///
/// 一時名の録画でも、書き終えたもの（`keep` がどの名前にも置けず、場所をエラーで知らせて残したもの）は
/// 作り直せないので消さない。書き終えたか確かめられない（読めない）録画も、同じ理由で消さない。
pub fn remove_leftovers(folder: &Path) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let Some((kind, writer)) = entry.file_name().to_str().and_then(writer_of) else {
            continue;
        };
        if alive(writer) {
            continue;
        }
        let path = entry.path();
        if kind == "mp4" && finished_video(&path).unwrap_or(true) {
            continue;
        }
        let _ = fs::remove_file(path);
    }
}

/// 書きかけの名前から、種類（`KINDS` のどれか、小文字）と書いていたプロセスの ID を読む。
/// `KINDS` の書きかけでなければ `None`。
fn writer_of(name: &str) -> Option<(&'static str, u32)> {
    let (rest, id) = name.strip_suffix(".partial")?.rsplit_once('.')?;
    let (_, kind) = rest.rsplit_once('.')?;
    let kind = KINDS
        .into_iter()
        .find(|known| kind.eq_ignore_ascii_case(known))?;
    Some((kind, process_id(id)?))
}

/// 一時名に入れたプロセス ID を読む。10 進の数字だけで `u32` に収まるときだけ（`+5` や空、桁あふれは
/// `None`。`str::parse` は先頭の `+` を受け付けるので、数字だけかを先に確かめる）。設定ファイルの
/// 一時ファイル（`config`）も同じ規則で読む。
pub fn process_id(id: &str) -> Option<u32> {
    let digits = !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit());
    digits.then(|| id.parse().ok()).flatten()
}

/// `path` が書き終えた MP4 か。MP4 は最上位のボックス（4 バイトの大きさと 4 バイトの種類で始まる）を
/// 並べたもので、録画を仕上げる（`Finalize`）ときに、再生に要る目次の `moov` ボックスを書く。
/// 先頭からボックスの見出しだけをたどり（映像のデータの中身は読まない）、ファイルに収まる `moov` が
/// あれば書き終えている。途中で止まった録画には `moov` がないか、ボックスがファイルの残りより大きい
/// （書いている途中の `mdat` か `moov`）。
fn finished_video(path: &Path) -> io::Result<bool> {
    use io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path)?;
    let length = file.metadata()?.len();
    let mut at = 0;
    while length - at >= 8 {
        file.seek(SeekFrom::Start(at))?;
        let mut header = [0; 8];
        file.read_exact(&mut header)?;
        let [a, b, c, d, kind @ ..] = header;
        // 大きさ 0（ファイルの終わりまで）は見出しより小さいものとして扱う。書いている途中の `mdat` で、
        // その後に `moov` はない。見出しより小さい大きさを拒むので、1 回ごとに 8 バイト以上進み、
        // たどるのは必ず終わる。
        let (size, header_size) = match u32::from_be_bytes([a, b, c, d]) {
            // 大きさ 1 は、続く 8 バイトに 64 ビットで書いてある。
            1 => {
                if length - at < 16 {
                    return Ok(false);
                }
                let mut large = [0; 8];
                file.read_exact(&mut large)?;
                (u64::from_be_bytes(large), 16)
            }
            size => (u64::from(size), 8),
        };
        if size < header_size || size > length - at {
            return Ok(false);
        }
        if kind == *b"moov" {
            return Ok(true);
        }
        at += size;
    }
    Ok(false)
}

/// プロセスが動いているか。開けないプロセス（もう無いか、別の利用者のもの）は書きかけの持ち主ではない。
/// テストも、終わったプロセスが残した作業用のフォルダーを見分けるのに使う。
pub fn alive(id: u32) -> bool {
    if id == std::process::id() {
        return true;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: BOOL, id: u32) -> HANDLE;
        fn GetExitCodeProcess(process: HANDLE, code: *mut u32) -> BOOL;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    // SAFETY: the handle is checked before use and closed once.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, id);
        if process.is_null() {
            return false;
        }
        let mut code = 0;
        let read = GetExitCodeProcess(process, &mut code) != 0;
        CloseHandle(process);
        read && code == STILL_ACTIVE
    }
}

/// 書き込みの 1 段階（`label`、対象は `path`）を終えた。
#[cfg(test)]
pub fn step(label: &str, path: &Path) {
    crate::test_support::crash::step(label, path);
}

/// 書き込みの 1 段階を終えた。製品のビルドでは何もしない。
#[cfg(not(test))]
#[inline(always)]
pub fn step(_label: &str, _path: &Path) {}

#[cfg(test)]
mod tests;
