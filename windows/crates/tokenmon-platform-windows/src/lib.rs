//! The Windows adapters of TokenMon's OS services.
//!
//! The traits, state machines, fakes and every portable decision live in
//! `tokenmon-platform` (shared with the other desktop ports); this crate holds
//! only what talks to Win32, WinRT, DPAPI, the registry or WebView2, plus the
//! Windows-specific update logic (PE version checks, MSI hand-off). It is the
//! only crate allowed to contain `unsafe` code; every unsafe block carries a
//! `// SAFETY:` comment.
//!
//! The crate compiles on every host so its portable parts (SDDL strings, the
//! PE parser, `msiexec` arguments, the registry value codec) are tested on
//! macOS and Linux too; the OS calls are `cfg(windows)`.

pub mod appearance;
pub mod install;
pub mod notify;
pub mod power;
pub mod secrets;
pub mod shell;
pub mod webauth;
