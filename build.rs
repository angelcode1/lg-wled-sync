// Minimal C ABI bridge for webOS native vtcapture logging.
// Native webOS uses the variadic printLog symbol, which stable Rust cannot
// implement directly. This build script uses the existing host/cross C tools:
// no new Cargo crate or sysroot is required.
use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=native/lg_printlog.c");
    let target = env::var("TARGET").expect("Cargo must supply TARGET");
    if !target.contains("-linux-") {
        return;
    }
    let suffix = target.replace('-', "_");
    let cc = env::var(format!("CC_{suffix}"))
        .or_else(|_| env::var("CC"))
        .unwrap_or_else(|_| "cc".into());
    let ar = env::var(format!("AR_{suffix}"))
        .or_else(|_| env::var("AR"))
        .unwrap_or_else(|_| "ar".into());
    let out = PathBuf::from(env::var("OUT_DIR").expect("Cargo must supply OUT_DIR"));
    let object = out.join("lg_printlog.o");
    let archive = out.join("liblg_printlog.a");
    let status = Command::new(&cc)
        .args([
            "-std=c11",
            "-fPIC",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-c",
            "native/lg_printlog.c",
            "-o",
        ])
        .arg(&object)
        .status()
        .expect("C compiler unavailable for LG vtcapture ABI shim");
    assert!(
        status.success(),
        "LG vtcapture C ABI shim failed compilation"
    );
    let status = Command::new(&ar)
        .arg("crs")
        .arg(&archive)
        .arg(&object)
        .status()
        .expect("C archiver unavailable for LG vtcapture ABI shim");
    assert!(status.success(), "LG vtcapture C ABI archive failed");
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=lg_printlog");
    // LG's libvtcapture resolves printLog from the *global ELF symbol table*
    // during dlopen. Ensure it remains in .dynsym, including under --release.
    println!("cargo:rustc-link-arg=-Wl,--export-dynamic");
}
