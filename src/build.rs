use std::{env, path::PathBuf, process::Command};

fn resource_compiler() -> PathBuf {
    println!("cargo:rerun-if-env-changed=RC");
    if let Some(compiler) = env::var_os("RC") {
        return compiler.into();
    }
    if let Some(path) = env::var_os("PATH") {
        if let Some(compiler) = env::split_paths(&path)
            .map(|directory| directory.join("rc.exe"))
            .find(|compiler| compiler.is_file())
        {
            return compiler;
        }
    }
    let sdk =
        PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK installation root"))
            .join("Windows Kits/10/bin");
    let mut versions: Vec<_> = std::fs::read_dir(sdk)
        .expect("Install the Windows SDK resource compiler (rc.exe)")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    versions.sort();
    let arch = if env::var("HOST").unwrap_or_default().starts_with("aarch64") {
        "arm64"
    } else {
        "x64"
    };
    versions
        .into_iter()
        .rev()
        .map(|version| version.join(arch).join("rc.exe"))
        .find(|compiler| compiler.is_file())
        .expect("Install the Windows SDK resource compiler (rc.exe), or set RC to its path")
}

fn main() {
    // Visual styles for the settings window controls (comctl32 v6).
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("app.manifest");
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTINPUT:{}",
            manifest.display()
        );
        println!("cargo:rerun-if-changed=app.manifest");
        let resource = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("app.res");
        let status = Command::new(resource_compiler())
            .args(["/nologo", "/fo"])
            .arg(&resource)
            .arg("app.rc")
            .current_dir(env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .status()
            .expect("Run the Windows SDK resource compiler");
        assert!(
            status.success(),
            "Compiling the application icon resource failed"
        );
        println!("cargo:rustc-link-arg-bins={}", resource.display());
        println!("cargo:rerun-if-changed=app.rc");
        println!("cargo:rerun-if-changed=app.ico");
    }
}
