//! The WebView2 sign-in window.
//!
//! The sign-in state machine, navigation gate, cookie allowlist, profile
//! folders and the headless driver are portable and live in
//! `tokenmon_platform::webauth`. `webview2` (`cfg(windows)`) is a thin adapter:
//! it owns a native Win32 window with the header, navigation bar and footer as
//! native controls, hosts WebView2 in the body and executes the machine's
//! actions. It contains no decisions.

#[cfg(windows)]
pub mod webview2;
#[cfg(windows)]
mod window_icon;

#[cfg(windows)]
pub use webview2::{WindowsWebAuthHost, webview2_available};
