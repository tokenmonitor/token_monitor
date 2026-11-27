//! The testable core of the `TokenMon.exe` entry point.
//!
//! `main.rs` only names the real collaborators (data directory, the exe's own
//! resource checks, the headless smoke cycle, the Win32 shell); the decisions
//! about which logger to install and which exit code a run earns live here so
//! they run in-process on every host.

pub mod exe_resources;
pub mod startup;
