//! Exposes the compile target triple to the program as `BUILD_TARGET`, so the
//! self-update command can pick the matching release asset (e.g.
//! `cav-x86_64-apple-darwin`).

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=BUILD_TARGET={target}");
}
