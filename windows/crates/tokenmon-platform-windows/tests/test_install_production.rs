//! The production wiring of the Windows update installer: the release channel
//! it follows and the pair the update service is built from.

use std::path::PathBuf;

use tokenmon_core::update::UpdateInstaller;
use tokenmon_platform_windows::install::{
    WINDOWS_ASSET_PREFIX, WINDOWS_TAG_PREFIX, WindowsUpdateInstaller, windows_release_source,
};

/// The channel only follows `windows-v*` tags and `tokenmon-windows-*` assets.
#[test]
fn test_release_source_is_the_windows_channel() {
    let source = windows_release_source();
    assert_eq!(source.tag_prefix, WINDOWS_TAG_PREFIX);
    assert_eq!(source.asset_prefix, WINDOWS_ASSET_PREFIX);
    assert_eq!(WINDOWS_TAG_PREFIX, "windows-v");
    assert_eq!(WINDOWS_ASSET_PREFIX, "tokenmon-windows-");
}

/// `production` builds an installer for the Windows channel over the given exe
/// without touching the network, and a running exe in a writable folder can be
/// replaced.
#[test]
fn test_production_installer_wraps_the_running_exe() {
    let dir = tempfile::tempdir().expect("tempdir");
    let exe: PathBuf = dir.path().join("TokenMon.exe");
    std::fs::write(&exe, b"MZ").expect("write exe");
    let installer =
        WindowsUpdateInstaller::production(windows_release_source(), exe, "TokenMon-test")
            .expect("http stack starts");
    assert!(installer.can_replace_running_app());
}

/// `production_parts` returns the Windows channel and a working installer.
#[test]
fn test_production_parts_pair_the_channel_with_an_installer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let exe = dir.path().join("TokenMon.exe");
    std::fs::write(&exe, b"MZ").expect("write exe");
    let (source, installer) =
        WindowsUpdateInstaller::production_parts(exe, "1.2.3").expect("parts");
    assert_eq!(source.tag_prefix, WINDOWS_TAG_PREFIX);
    assert!(installer.can_replace_running_app());
    // A temp folder is never the MSI's recorded install folder.
    assert!(!installer.installed_by_msi());
}
