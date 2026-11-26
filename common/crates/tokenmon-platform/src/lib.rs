//! OS services behind traits.
//!
//! Every service has a trait, an in-memory fake and the portable logic around
//! it (state machines, codecs, parsers, the vault), so the rest of the
//! workspace tests on every OS. The OS adapters live in the per-OS crates
//! (`tokenmon-platform-windows`); this crate contains no `unsafe` code.

pub mod install;
pub mod notify;
pub mod power;
pub mod secrets;
pub mod shell;
pub mod webauth;
