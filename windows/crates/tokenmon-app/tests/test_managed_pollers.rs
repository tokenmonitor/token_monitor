//! The production pollers' lifecycle controls and the update loop, with
//! nothing signed in so no network is touched.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::env;
use tokenmon_app::providers::{ProviderDeps, build_registry};
use tokenmon_app::updates::UpdateService;
use tokenmon_core::storage::AppPaths;
use tokenmon_core::update::UrlOpener;
use tokenmon_core::{Clock, ProviderId};
use tokenmon_provider_opencode::db::DataDirEnv;

struct NoOpener;
impl UrlOpener for NoOpener {
    fn open(&self, _url: &str) {}
}

/// Every production poller accepts the lifecycle controls and refreshes on
/// demand: the conditional refresh runs once when none happened yet and is
/// skipped while the last one is fresh.
#[tokio::test]
async fn test_every_production_poller_obeys_the_lifecycle_controls() {
    let e = env();
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path());
    let clock: Arc<dyn Clock> = e.dyn_clock();
    let registry = build_registry(&ProviderDeps {
        settings: &e.settings,
        sessions: &e.sessions,
        paths: &paths,
        clock: &clock,
        version: Some("0.0.0-test"),
        home: Some(dir.path().to_path_buf()),
        claude_config_dir: None,
        opencode_env: DataDirEnv::default(),
    })
    .expect("registry builds");
    for entry in registry.entries() {
        let p = &entry.poller;
        p.wake();
        p.pause();
        p.resume();
        p.refresh_now_if_older_than(Duration::from_secs(15)).await;
        let first = p.last_refreshed_at();
        p.refresh_now_if_older_than(Duration::from_secs(15)).await;
        assert_eq!(
            p.last_refreshed_at(),
            first,
            "{:?}: a fresh refresh is not repeated",
            entry.provider
        );
        p.stop();
    }
    assert_eq!(registry.entries().len(), ProviderId::USAGE_PROVIDERS.len());
}

/// With the update switch off, the periodic loop keeps running without ever
/// checking the feed, until aborted.
#[tokio::test(start_paused = true)]
async fn test_update_loop_idles_when_checks_are_disabled() {
    let e = env();
    e.settings.set_checks_for_updates(false);
    let service = UpdateService::production(
        &e.settings,
        "1.2.3",
        "TokenMon.exe".into(),
        Arc::new(NoOpener),
    )
    .expect("updates");
    let task = service.spawn_loop(&tokio::runtime::Handle::current());
    tokio::time::sleep(Duration::from_hours(24)).await;
    assert!(!task.is_finished(), "the loop keeps running");
    assert!(service.checker.state().last_error.is_none());
    task.abort();
    assert!(task.await.expect_err("aborted").is_cancelled());
}
