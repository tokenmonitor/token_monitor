//! Failure handling and trait accessors of `GrokProviderPoller`: a failed
//! fetch surfaces as the provider's last error and feeds the loop's backoff
//! (honoring `Retry-After`), and the accessors mirror the usage poller.
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{clock_at, utc};
use tokenmon_core::polling::testing::ManualScheduler;
use tokenmon_core::settings::Settings;
use tokenmon_core::{
    AuthEventListener, AuthSession, AuthSessionState, BoxFuture, ProviderError,
    ProviderErrorContext, ProviderId, ProviderUsagePoller, SessionGeneration, UsageError,
};
use tokenmon_provider_grok::history::GrokHistoryStore;
use tokenmon_provider_grok::models::WeeklyUsageSnapshot;
use tokenmon_provider_grok::poller::{GrokFetch, GrokUsagePoller};
use tokenmon_provider_grok::provider_poller::GrokProviderPoller;

struct FakeAuth(Mutex<AuthSessionState>);

impl AuthSession for FakeAuth {
    fn cookie_header(&self) -> Option<String> {
        Some("sso=live".to_owned())
    }
    fn state(&self) -> AuthSessionState {
        self.0.lock().unwrap().clone()
    }
    fn record_auth_failure(&self, reason: &str) -> bool {
        self.0.lock().unwrap().record_auth_failure(reason)
    }
    fn record_auth_success(&self) {
        self.0.lock().unwrap().record_auth_success();
    }
    fn is_current(&self, generation: SessionGeneration) -> bool {
        self.0.lock().unwrap().is_current(generation)
    }
    fn add_listener(&self, _listener: Arc<dyn AuthEventListener>) {}
}

struct ScriptedFetch(Mutex<Result<WeeklyUsageSnapshot, ProviderError>>);

impl GrokFetch for ScriptedFetch {
    fn fetch(
        &self,
        _cookie: String,
        _email: Option<String>,
    ) -> BoxFuture<'_, Result<WeeklyUsageSnapshot, ProviderError>> {
        let result = self.0.lock().unwrap().clone();
        Box::pin(async move { result })
    }
}

fn rig(
    result: Result<WeeklyUsageSnapshot, ProviderError>,
) -> (GrokProviderPoller, ManualScheduler) {
    let mut state = AuthSessionState::default();
    state.mark_signed_in(Some("me@x.co".to_owned()));
    let auth = Arc::new(FakeAuth(Mutex::new(state)));
    let clock = clock_at(utc(2026, 7, 15, 12, 0));
    let inner = Arc::new(GrokUsagePoller::new(
        auth,
        GrokHistoryStore::in_memory(clock.clone()),
        clock,
        Arc::new(ScriptedFetch(Mutex::new(result))),
        None,
    ));
    let settings = Settings::default().with_selected_provider(ProviderId::Grok);
    let scheduler = ManualScheduler::new();
    let sched = scheduler.clone();
    let provider = GrokProviderPoller::new(inner, Arc::new(move || settings.clone()), move |b| {
        b.clock(sched.clock()).sleeper(Arc::new(sched.clone()))
    });
    (provider, scheduler)
}

/// A rate-limited fetch is the provider's last error and pushes the next
/// refresh out by at least the server's `Retry-After`.
#[tokio::test]
async fn test_rate_limited_failure_defers_next_poll_by_retry_after() {
    let error = ProviderError::from_usage(
        UsageError::RateLimited {
            retry_after: Some(7200.0),
        },
        ProviderErrorContext::Grok,
    );
    let (provider, _scheduler) = rig(Err(error.clone()));
    provider.refresh_now().await;
    assert_eq!(provider.last_error(), Some(error));
    assert_eq!(provider.headline_used_percent(), None);
    assert_eq!(provider.last_refreshed_at(), None);
    let delay = provider.polling_loop().delay_until_due().unwrap();
    assert!(delay >= Duration::from_secs(7200), "delay was {delay:?}");
}

/// A plain network failure keeps the error on the provider and still backs
/// the loop off (the next poll is not due immediately).
#[tokio::test]
async fn test_network_failure_backs_off_without_retry_after() {
    let error = ProviderError::Network(ProviderErrorContext::Grok, "boom".to_owned());
    let (provider, _scheduler) = rig(Err(error.clone()));
    provider.refresh_now().await;
    assert_eq!(provider.last_error(), Some(error));
    assert!(provider.polling_loop().delay_until_due().unwrap() > Duration::ZERO);
}

/// Success exposes the refresh time and the headline reset through the trait
/// and clears any earlier error.
#[tokio::test]
async fn test_success_exposes_refresh_time_and_headline_reset() {
    let reset = utc(2026, 7, 20, 18, 0);
    let snapshot =
        WeeklyUsageSnapshot::new(utc(2026, 7, 15, 12, 0), 12.5).with_resets_at(Some(reset));
    let (provider, _scheduler) = rig(Ok(snapshot));
    provider.refresh_now().await;
    assert_eq!(provider.last_error(), None);
    assert_eq!(provider.last_refreshed_at(), Some(utc(2026, 7, 15, 12, 0)));
    assert_eq!(provider.headline_resets_at(), Some(reset));
    assert_eq!(provider.headline_used_percent(), Some(12.5));
}

/// `settings_changed` wakes the loop so a re-evaluated interval takes effect.
#[tokio::test]
async fn test_settings_changed_wakes_a_running_loop() {
    let (provider, scheduler) = rig(Ok(WeeklyUsageSnapshot::new(utc(2026, 7, 15, 12, 0), 1.0)));
    provider.start();
    scheduler.settle().await;
    let before = scheduler.cancelled_count();
    provider.settings_changed();
    scheduler.settle().await;
    assert!(scheduler.cancelled_count() > before);
    provider.stop();
}
