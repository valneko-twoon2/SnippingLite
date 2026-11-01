//! 書きかけを書いたのが、この PC のこの利用者のプロセスだとわかるか。
//!
//! 書きかけの名前のプロセス ID は、書いた PC の中でだけ意味を持つ。`staging::alive` はこの PC の
//! プロセスを、この利用者の権限で開いて確かめるので、
//!
//! - ネットワークの場所（ほかの PC の Snipping Lite も書く）では、ほかの PC で書いている途中の
//!   プロセスが、この PC には無いので動いていないように見える。
//! - ほかの利用者が持ち主のファイル（Public など、利用者の間で共有したフォルダー）では、書いている
//!   途中のほかの利用者のプロセスを開けないので、動いていないように見える。
//!
//! そうしたファイルは、`remove_leftovers` が最後に書かれた時刻でも確かめる。どちらとも確かめられない
//! ときは、この PC のこの利用者のものとはみなさない。

use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path, ptr::null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HANDLE};

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

/// `folder` が、この PC につながったドライブ（内蔵、USB、光学、RAM ディスク）にあるか。ネットワークの
/// ドライブと共有フォルダー（`\\server\share`）、確かめられない場所は、ないとみなす。
pub(super) fn on_this_pc(folder: &Path) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetVolumePathNameW(path: *const u16, root: *mut u16, length: u32) -> BOOL;
        fn GetDriveTypeW(root: *const u16) -> u32;
    }
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_CDROM: u32 = 5;
    const DRIVE_RAMDISK: u32 = 6;
    let path = wide(folder);
    // ドライブの根は `folder` より長くならない。
    let mut root = vec![0u16; path.len().max(261)];
    let Ok(length) = u32::try_from(root.len()) else {
        return false;
    };
    // SAFETY: `path` is nul-terminated and `root` holds `length` characters; the root that
    // GetVolumePathNameW writes is nul-terminated and outlives GetDriveTypeW.
    unsafe {
        if GetVolumePathNameW(path.as_ptr(), root.as_mut_ptr(), length) == 0 {
            return false;
        }
        matches!(
            GetDriveTypeW(root.as_ptr()),
            DRIVE_REMOVABLE | DRIVE_FIXED | DRIVE_CDROM | DRIVE_RAMDISK
        )
    }
}

/// `file` の持ち主が、このプロセスが作るファイルの持ち主（トークンの既定の持ち主。ふつうは利用者で、
/// 管理者として動いていれば Administrators）と同じか。どちらかを読めなければ、違うとみなす。
pub(super) fn owned_here(file: &Path) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> HANDLE;
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process: HANDLE, access: u32, token: *mut HANDLE) -> BOOL;
        fn GetTokenInformation(
            token: HANDLE,
            class: i32,
            information: *mut c_void,
            length: u32,
            returned: *mut u32,
        ) -> BOOL;
        fn GetNamedSecurityInfoW(
            name: *const u16,
            kind: i32,
            information: u32,
            owner: *mut *mut c_void,
            group: *mut *mut c_void,
            dacl: *mut *mut c_void,
            sacl: *mut *mut c_void,
            descriptor: *mut *mut c_void,
        ) -> u32;
        fn EqualSid(a: *const c_void, b: *const c_void) -> BOOL;
    }
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_OWNER: i32 = 4;
    const SE_FILE_OBJECT: i32 = 1;
    const OWNER_SECURITY_INFORMATION: u32 = 1;
    let name = wide(file);
    // TOKEN_OWNER（SID へのポインター）と、その後ろに置かれる SID（最大 68 バイト）が収まる。
    let mut token_owner = [0usize; 16];
    let size = std::mem::size_of_val(&token_owner) as u32;
    // SAFETY: the token is closed once. The owner of the token points into `token_owner`, which
    // outlives the comparison. The security descriptor that holds the owner of the file is freed
    // once, after the comparison.
    unsafe {
        let mut token = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut returned = 0;
        let read = GetTokenInformation(
            token,
            TOKEN_OWNER,
            token_owner.as_mut_ptr().cast(),
            size,
            &mut returned,
        ) != 0;
        CloseHandle(token);
        if !read {
            return false;
        }
        let mine = token_owner.as_ptr().cast::<*const c_void>().read();
        let mut owner = null_mut();
        let mut descriptor = null_mut();
        if GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        ) != 0
        {
            return false;
        }
        let same = !owner.is_null() && EqualSid(owner, mine) != 0;
        LocalFree(descriptor);
        same
    }
}
