//! Beta stand-ins for the platform services the shared Settings UI asks for:
//! launch at login, the desktop verb and the updater. M0 has no real
//! implementations; they report "not available" instead of pretending, so the
//! Settings window shows an honest state.

use std::sync::Arc;

use tokenmon_core::provider::BoxFuture;
use tokenmon_core::update::{
    InstallError, InstallOutcome, InstallRequest, UpdateInstaller, UrlOpener,
};
use tokenmon_platform::shell::{LoginItemService, LoginItemStatus, ShellError, ShellVerbService};

/// Message shown when a Settings toggle has no macOS implementation yet.
pub const UNAVAILABLE: &str = "not available in the Rust beta yet";

/// Launch at login: always "not registered"; registering reports the gap.
pub struct BetaLoginItem;

impl LoginItemService for BetaLoginItem {
    fn register(&self) -> Result<(), ShellError> {
        Err(ShellError::Registry(UNAVAILABLE.into()))
    }
    fn unregister(&self) -> Result<(), ShellError> {
        Ok(())
    }
    fn status(&self) -> Result<LoginItemStatus, ShellError> {
        Ok(LoginItemStatus::NotRegistered)
    }
}

/// The Windows desktop context-menu verb has no macOS counterpart.
pub struct BetaDesktopVerb;

impl ShellVerbService for BetaDesktopVerb {
    fn install(&self) -> Result<(), ShellError> {
        Err(ShellError::Registry(UNAVAILABLE.into()))
    }
    fn uninstall(&self) -> Result<(), ShellError> {
        Ok(())
    }
    fn is_installed(&self) -> Result<bool, ShellError> {
        Ok(false)
    }
}

/// An installer that never replaces the app (updates arrive in M3).
pub struct BetaInstaller;

impl UpdateInstaller for BetaInstaller {
    fn can_replace_running_app(&self) -> bool {
        false
    }

    fn install(
        &self,
        _request: InstallRequest,
    ) -> BoxFuture<'_, Result<InstallOutcome, InstallError>> {
        Box::pin(async { Err(InstallError::NotWritable) })
    }
}

/// Opens URLs through a caller-supplied launcher; only `https://` URLs pass.
pub struct HttpsOpener {
    launch: Arc<dyn Fn(&str) + Send + Sync>,
}

impl HttpsOpener {
    /// An opener that hands accepted URLs to `launch`.
    pub fn new(launch: Arc<dyn Fn(&str) + Send + Sync>) -> Self {
        Self { launch }
    }
}

impl UrlOpener for HttpsOpener {
    fn open(&self, url: &str) {
        if url.starts_with("https://") {
            (self.launch)(url);
        }
    }
}
