//! What the smoke run reports: a stuck refresh and an incomplete registry
//! must fail the run, a full finished cycle passes.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokenmon_app::model::AppModel;
use tokenmon_app::poller::ManagedPoller;
use tokenmon_app::registry::{ProviderEntry, ProviderRegistry, SessionKey};
use tokenmon_app::smoke::{SmokeReport, run_refresh_cycle};
use tokenmon_app::testing::FakePoller;
use tokenmon_core::settings::{AppSettings, SettingsFile};
use tokenmon_core::{BoxFuture, ProviderError, ProviderId, ProviderUsagePoller};

/// A poller whose refresh never finishes.
struct Stuck(Arc<FakePoller>);

impl ProviderUsagePoller for Stuck {
    fn provider(&self) -> ProviderId {
        self.0.provider()
    }
    fn set_menu_open(&self, open: bool) {
        self.0.set_menu_open(open);
    }
    fn last_refreshed_at(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn last_error(&self) -> Option<ProviderError> {
        None
    }
    fn headline_used_percent(&self) -> Option<f64> {
        None
    }
    fn headline_resets_at(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn alert_account(&self) -> Option<String> {
        None
    }
    fn start(&self) {}
    fn stop(&self) {}
    fn refresh_now(&self) -> BoxFuture<'_, ()> {
        Box::pin(std::future::pending())
    }
    fn clear_snapshot(&self) {}
}

impl ManagedPoller for Stuck {
    fn wake(&self) {}
    fn pause(&self) {}
    fn resume(&self) {}
    fn refresh_now_if_older_than(&self, _age: Duration) -> BoxFuture<'_, ()> {
        Box::pin(std::future::pending())
    }
}

fn model_with(poller: Arc<dyn ManagedPoller>, provider: ProviderId) -> Arc<AppModel> {
    let session = Arc::new(0_u8);
    let entry = ProviderEntry {
        provider,
        poller,
        session: SessionKey::of(&session),
    };
    std::mem::forget(session);
    AppModel::new(
        ProviderRegistry::new(vec![entry]),
        Arc::new(AppSettings::new(Arc::new(SettingsFile::in_memory()))),
        tokio::runtime::Handle::current(),
    )
}

/// A refresh that outlives the timeout yields `completed == false`, the run
/// fails, and the model is still quit.
#[tokio::test]
async fn test_stuck_refresh_times_out_and_fails_the_report() {
    let model = model_with(
        Arc::new(Stuck(FakePoller::new(ProviderId::Grok))),
        ProviderId::Grok,
    );
    let report = run_refresh_cycle(&model, Duration::from_millis(20)).await;
    assert!(!report.completed);
    assert_eq!(report.providers, vec![ProviderId::Grok]);
    assert!(!report.passed());
    assert!(model.has_quit());
}

/// A finished cycle over an incomplete registry still fails: every usage
/// provider must be registered.
#[tokio::test]
async fn test_finished_cycle_with_missing_providers_does_not_pass() {
    let model = model_with(FakePoller::new(ProviderId::Grok), ProviderId::Grok);
    let report = run_refresh_cycle(&model, Duration::from_secs(5)).await;
    assert!(report.completed);
    assert!(!report.passed());
}

/// The pass rule is the conjunction of "completed" and "full registry".
#[test]
fn test_report_passes_only_when_complete_and_fully_registered() {
    let full = ProviderId::USAGE_PROVIDERS.to_vec();
    let report = |providers: Vec<ProviderId>, completed| SmokeReport {
        providers,
        completed,
    };
    assert!(report(full.clone(), true).passed());
    assert!(!report(full, false).passed());
}
