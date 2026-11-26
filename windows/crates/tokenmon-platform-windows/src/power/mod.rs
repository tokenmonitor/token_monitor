//! The Windows sleep/wake source: a hidden top-level window for
//! `WM_POWERBROADCAST` and the connectivity-hint subscription.
//!
//! The gate, the event decoder and the fakes are portable and live in
//! `tokenmon_platform::power`.

#[cfg(windows)]
pub mod windows;
