//! Identity of the Rust beta bundle and where it keeps its data.
//!
//! The beta never shares a bundle id, data directory or Keychain item with the
//! Swift app (issue #89 section 2.4), so both can run and be uninstalled
//! independently.

use std::path::{Path, PathBuf};

/// Bundle identifier of the beta (the Swift app is `com.modelmonitor.app`).
pub const BUNDLE_ID: &str = "com.modelmonitor.app.rust";

/// Name shown in Finder, the Dock-less app switcher and Activity Monitor.
pub const DISPLAY_NAME: &str = "TokenMon (Rust beta)";

/// File name of the executable inside `Contents/MacOS`.
pub const EXECUTABLE_NAME: &str = "tokenmon-macos";

/// Folder under `~/Library/Application Support` that holds the beta's data.
pub const DATA_DIR_NAME: &str = "TokenMon-RustBeta";

/// The base directory handed to `AppPaths::new` for `home`.
///
/// Note: `AppPaths::directory()` appends its own fixed `TokenMon` folder, so
/// the files end up in `<base>/TokenMon` (a gap recorded in the M0 report).
///
/// ```
/// use std::path::Path;
/// let base = tokenmon_macos::bundle::data_base_dir(Path::new("/Users/me"));
/// assert!(base.ends_with("Library/Application Support/TokenMon-RustBeta"));
/// ```
pub fn data_base_dir(home: &Path) -> PathBuf {
    home.join("Library")
        .join("Application Support")
        .join(DATA_DIR_NAME)
}

/// The `Info.plist` of the beta: an agent app (no Dock icon, no main menu)
/// with the beta's identity. `version` is the workspace version.
///
/// ```
/// let plist = tokenmon_macos::bundle::info_plist("1.2.3");
/// assert!(plist.contains("<key>LSUIElement</key>\n\t<true/>"));
/// assert!(plist.contains("com.modelmonitor.app.rust"));
/// ```
pub fn info_plist(version: &str) -> String {
    let rows: [(&str, String); 11] = [
        ("CFBundleIdentifier", plist_string(BUNDLE_ID)),
        ("CFBundleName", plist_string(DISPLAY_NAME)),
        ("CFBundleDisplayName", plist_string(DISPLAY_NAME)),
        ("CFBundleExecutable", plist_string(EXECUTABLE_NAME)),
        ("CFBundlePackageType", plist_string("APPL")),
        ("CFBundleShortVersionString", plist_string(version)),
        ("CFBundleVersion", plist_string(version)),
        ("LSMinimumSystemVersion", plist_string("14.0")),
        ("NSHighResolutionCapable", "<true/>".to_owned()),
        ("LSUIElement", "<true/>".to_owned()),
        (
            "NSHumanReadableCopyright",
            plist_string("MIT, TokenMon contributors"),
        ),
    ];
    let body: String = rows
        .iter()
        .map(|(k, v)| format!("\t<key>{k}</key>\n\t{v}\n"))
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n{body}</dict>\n</plist>\n"
    )
}

fn plist_string(value: &str) -> String {
    let escaped = value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!("<string>{escaped}</string>")
}
