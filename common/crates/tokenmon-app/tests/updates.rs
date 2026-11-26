//! The update service wires up without touching the network.

use std::sync::Arc;

use tokenmon_app::testing::RefusingInstaller;
use tokenmon_app::updates::UpdateService;
use tokenmon_core::settings::{AppSettings, SettingsFile};
use tokenmon_core::update::{ReleaseSource, UrlOpener};

struct NoOpener;

impl UrlOpener for NoOpener {
    fn open(&self, _url: &str) {}
}

/// Building the service starts nothing: the checker has no release and is not
/// checking until the loop runs.
#[tokio::test]
async fn test_update_service_starts_idle() {
    let settings = Arc::new(AppSettings::new(Arc::new(SettingsFile::in_memory())));
    let service = UpdateService::with_http_feed(
        ReleaseSource::new("windows-v", "tokenmon-windows-"),
        &settings,
        "1.0.0",
        Arc::new(NoOpener),
        Arc::new(RefusingInstaller),
    )
    .expect("builds");
    let state = service.checker.state();
    assert!(state.available_release.is_none());
    assert!(!state.is_checking);
    assert!(service.checker.settings_changed());
    settings.set_checks_for_updates(false);
    assert!(!service.checker.settings_changed());
}
