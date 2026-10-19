//! Windows gives the main thread 1 MB of stack; clap's parser for the whole
//! command tree overflows it in debug builds. Ask for 8 MB, as on Linux.
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("windows-msvc") {
        println!("cargo:rustc-link-arg-bins=/STACK:8388608");
    } else if target.contains("windows-gnu") {
        println!("cargo:rustc-link-arg-bins=-Wl,--stack,8388608");
    }
}
