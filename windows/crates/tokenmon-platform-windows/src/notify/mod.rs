//! WinRT toast support: first-run AppUserModelID registration and the
//! Windows toast backend.
//!
//! The toast model, the [`ToastNotifier`](tokenmon_platform::notify::ToastNotifier)
//! and the fake backend are portable and live in `tokenmon_platform::notify`.
//!
//! * [`aumid`]: first-run registration of the AppUserModelID in HKCU, through
//!   the shell [`Registry`](tokenmon_platform::shell::Registry) trait.
//! * `windows` (`cfg(windows)`): the WinRT backend.

pub mod aumid;
#[cfg(windows)]
pub mod windows;

pub use aumid::{AumidRegistration, AumidStatus};
