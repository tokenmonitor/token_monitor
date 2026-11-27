//! The resources the shipped `TokenMon.exe` carries about itself, checked by
//! `--smoke` (CI runs the real exe): the version resource the updater trusts
//! (#77) and the application icon (#81). Only checked for the MSVC build the
//! releases ship; `build.rs` writes both.

/// Whether the running exe carries the version resource and the icon this
/// build declares. Logs the first problem. Always `true` off Windows/MSVC.
pub fn own_resources_match(version: &str) -> bool {
    resources_match_with(
        version,
        &own_version_resource_matches,
        &own_icon_resource_present,
    )
}

/// [`own_resources_match`] with the two checks supplied, so the failure
/// reporting runs without a real exe that lacks a resource.
pub fn resources_match_with(
    version: &str,
    version_ok: &dyn Fn(&str) -> bool,
    icon_ok: &dyn Fn() -> bool,
) -> bool {
    if !version_ok(version) {
        tracing::error!("smoke: the exe's version resource does not match {version}");
        return false;
    }
    if !icon_ok() {
        tracing::error!("smoke: the exe has no application icon (RT_GROUP_ICON) resource");
        return false;
    }
    true
}

/// The updater trusts the version resource of a downloaded exe (#77), so every
/// shipped exe must carry one equal to the build's own version (`build.rs`).
#[cfg(all(windows, target_env = "msvc"))]
fn own_version_resource_matches(version: &str) -> bool {
    use tokenmon_core::update::AppVersion;

    let declared = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::File::open(p).ok())
        .and_then(tokenmon_platform_windows::install::read_pe_version);
    declared.is_some() && declared == AppVersion::parse(version)
}

#[cfg(not(all(windows, target_env = "msvc")))]
fn own_version_resource_matches(_version: &str) -> bool {
    true
}

/// Without an icon group the taskbar, Alt+Tab and Explorer show the generic
/// application icon (#81); `build.rs` embeds `tokenmon.ico` as group id 1.
#[cfg(all(windows, target_env = "msvc"))]
fn own_icon_resource_present() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::File::open(p).ok())
        .is_some_and(tokenmon_platform_windows::install::pe_has_icon_group)
}

#[cfg(not(all(windows, target_env = "msvc")))]
fn own_icon_resource_present() -> bool {
    true
}
