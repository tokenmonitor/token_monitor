//! The beta's bundle identity never collides with the Swift app's.

use std::path::Path;

use tokenmon_macos::bundle::{
    BUNDLE_ID, DATA_DIR_NAME, DISPLAY_NAME, EXECUTABLE_NAME, data_base_dir, info_plist,
};

/// The beta has its own bundle id (the Swift app's is `com.modelmonitor.app`).
#[test]
fn test_bundle_id_differs_from_the_swift_app() {
    assert_eq!(BUNDLE_ID, "com.modelmonitor.app.rust");
    assert_ne!(BUNDLE_ID, "com.modelmonitor.app");
}

/// The data directory is the beta's own folder under Application Support.
#[test]
fn test_data_dir_is_under_application_support_and_not_swifts() {
    let base = data_base_dir(Path::new("/Users/me"));
    assert_eq!(
        base,
        Path::new("/Users/me/Library/Application Support").join(DATA_DIR_NAME)
    );
    assert_ne!(DATA_DIR_NAME, "TokenMon");
}

/// The plist carries the identity, the agent-app flag and the version.
#[test]
fn test_info_plist_carries_identity_agent_flag_and_version() {
    let plist = info_plist("9.8.7");
    for needle in [
        "<string>com.modelmonitor.app.rust</string>",
        &format!("<string>{DISPLAY_NAME}</string>"),
        &format!("<string>{EXECUTABLE_NAME}</string>"),
        "<key>LSUIElement</key>\n\t<true/>",
        "<string>9.8.7</string>",
    ] {
        assert!(plist.contains(needle), "missing {needle}");
    }
}

/// Values are XML-escaped so a version string cannot break the plist.
#[test]
fn test_info_plist_escapes_markup_in_values() {
    let plist = info_plist("1<2&3");
    assert!(plist.contains("<string>1&lt;2&amp;3</string>"));
    assert!(!plist.contains("1<2"));
}
