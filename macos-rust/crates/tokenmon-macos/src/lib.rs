//! TokenMon (Rust beta) for macOS: the M0 spike of issue #89.
//!
//! Everything that can be decided without AppKit lives in the safe modules of
//! this library so it is tested on every host: the bundle identity
//! ([`bundle`]), credentials entered by hand ([`credentials`]), the per-process
//! vault key ([`secrets`]), the status-item layout and the tiny-skia status
//! renderer ([`status_layout`], [`skia_status`]) and the panel placement math
//! ([`placement`]). The AppKit, Slint and WebKit glue is in `shell`, compiled
//! on macOS only.

pub mod bundle;
pub mod compare;
pub mod credentials;
pub mod mock;
pub mod options;
pub mod placement;
pub mod secrets;
pub mod skia_status;
pub mod status_layout;

#[cfg(target_os = "macos")]
pub mod shell;
