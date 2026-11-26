//! The Windows half of the updater: process relaunch and `msiexec`, PE version
//! and signature checks, and the installer that ties them to the portable
//! download, archive and swap steps in `tokenmon_platform::install`.
//!
//! Nothing here is `cfg(windows)`-only: the logic works on any OS against temp
//! directories (the "running exe" is just a file), so it is tested on macOS and
//! Linux. Only the Win32 calls (`WinVerifyTrust`, `OpenProcess`, process
//! creation flags) are `cfg(windows)`.
//!
//! Flow for the portable zip: download to a staging directory (80 MiB cap,
//! trusted redirects only) -> SHA-256 must equal the GitHub digest -> extract
//! with size/symlink/path limits -> require a top-level `TokenMon.exe` ->
//! [`verify`] hook -> stage a copy beside the running exe -> rename the running
//! exe aside, rename the staged copy into place -> relaunch. Any failure leaves
//! the installed exe untouched. MSI releases are downloaded, verified and
//! handed to `msiexec` instead.

pub mod installer;
pub mod pe_version;
pub mod relaunch;
pub mod verify;

pub use installer::{
    WINDOWS_ASSET_PREFIX, WINDOWS_TAG_PREFIX, WindowsUpdateInstaller, windows_release_source,
};
pub use pe_version::{
    PackageVersionReader, SystemVersionReader, pe_has_icon_group, read_pe_version,
};
pub use relaunch::{
    CommandSpec, ProcessLauncher, StdLauncher, handle_startup_args, parse_wait_for_pid,
    wait_for_pid_exit,
};
pub use verify::{
    AuthenticodeVerifier, PeHeaderVerifier, SignaturePolicy, SignatureVerifier, verifier_for_policy,
};
