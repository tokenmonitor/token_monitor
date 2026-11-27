//! TokenMon (Rust beta) for macOS. Thin entry point: the app is in
//! `tokenmon_macos::shell` (macOS only); other hosts get a clear message so
//! the workspace still builds and tests everywhere.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    tokenmon_macos::shell::run(&std::env::args().collect::<Vec<_>>())
}

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    eprintln!("tokenmon-macos is a macOS application");
    ExitCode::FAILURE
}
