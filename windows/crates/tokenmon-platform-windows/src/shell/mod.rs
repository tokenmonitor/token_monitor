//! The Windows shell adapters: HKCU registry access, the named mutex
//! single-instance lock and the named-pipe "show" IPC.
//!
//! The traits, fakes, command-line parsing and the wire format are portable
//! and live in `tokenmon_platform::shell`.

#[cfg(windows)]
pub mod windows;
