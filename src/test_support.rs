//! Helpers shared by the unit tests.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// Points `APPDATA`, `LOCALAPPDATA` and `USERPROFILE` at a fresh folder for the length of a test.
/// The environment is shared by every test thread, so one test at a time holds it.
pub struct Sandbox {
    root: PathBuf,
    saved: [(&'static str, Option<OsString>); 3],
    _guard: MutexGuard<'static, ()>,
}

impl Sandbox {
    pub fn new() -> Sandbox {
        let guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "snipping-lite-sandbox-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let saved =
            ["APPDATA", "LOCALAPPDATA", "USERPROFILE"].map(|name| (name, std::env::var_os(name)));
        std::env::set_var("APPDATA", root.join("roaming"));
        std::env::set_var("LOCALAPPDATA", root.join("local"));
        std::env::set_var("USERPROFILE", root.join("profile"));
        Sandbox {
            root,
            saved,
            _guard: guard,
        }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        for (name, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
