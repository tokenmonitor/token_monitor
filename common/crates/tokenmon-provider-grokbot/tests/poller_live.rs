//! `GrokbotPollerConfig::live` wiring against a mock server, plus the poller's
//! observable in-flight state, refresh log line and listener lifetime rules.

#![allow(clippy::unwrap_used, clippy::assert_is_empty)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use chrono::{TimeZone, Utc};
use tokenmon_core::polling::PollOutcome;
use tokenmon_core::storage::{InMemoryStringStore, TextStore};
use tokenmon_core::{
    AuthEvent, AuthEventListener, AuthSession, AuthSessionState, CapturedSession, Clock,
    CredentialStoreError, FixedClock, ManualClock, ProviderAuthSession, ProviderError,
    ProviderErrorContext, ProviderUsagePoller, SessionGeneration,
};
use tokenmon_provider_cursor::CursorHttp;
use tokenmon_provider_grokbot::{
    BreakdownFetchFn, FetchFn, GrokbotEntitlement, GrokbotPollerConfig, GrokbotPublisher,
    GrokbotSnapshot, GrokbotUsageBreakdown, GrokbotUsageClient, GrokbotUsagePoller,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const COOKIE: &str = "WorkosCursorSessionToken=u1%3A%3Aa.b.c";

struct FakeSession {
    state: Mutex<AuthSessionState>,
    listeners: Mutex<Vec<Arc<dyn AuthEventListener>>>,
}

impl FakeSession {
    fn new() -> Arc<Self> {
        let mut state = AuthSessionState::default();
        state.mark_signed_in(Some("a@b.com".into()));
        Arc::new(Self {
            state: Mutex::new(state),
            listeners: Mutex::new(Vec::new()),
        })
    }

    fn fire(&self, event: AuthEvent) {
        let listeners = self.listeners.lock().unwrap().clone();
        for listener in listeners {
            listener.on_auth_event(event);
        }
    }
}

impl AuthSession for FakeSession {
    fn cookie_header(&self) -> Option<String> {
        Some(COOKIE.to_owned())
    }
    fn state(&self) -> AuthSessionState {
        self.state.lock().unwrap().clone()
    }
    fn record_auth_failure(&self, reason: &str) -> bool {
        self.state.lock().unwrap().record_auth_failure(reason)
    }
    fn record_auth_success(&self) {
        self.state.lock().unwrap().record_auth_success();
    }
    fn is_current(&self, generation: SessionGeneration) -> bool {
        self.state.lock().unwrap().is_current(generation)
    }
    fn add_listener(&self, listener: Arc<dyn AuthEventListener>) {
        self.listeners.lock().unwrap().push(listener);
    }
}

impl ProviderAuthSession for FakeSession {
    fn set_needs_sign_in(&self, needs: bool) {
        self.state.lock().unwrap().needs_sign_in = needs;
    }
    fn save_account_email(&self, _email: &str) {}
    fn note_account_identity(&self, _identity: &str) {}
    fn apply_refreshed_cookies(&self, _set_cookie_headers: &[String]) {}
    fn mark_session_invalid(&self, _reason: Option<&str>) {}
    fn sign_out(&self) {}
    fn save(&self, _cookie_header: &str) -> Result<(), CredentialStoreError> {
        Ok(())
    }
    fn adopt(&self, _captured: &CapturedSession) -> Result<(), CredentialStoreError> {
        Ok(())
    }
    fn refresh_from_disk(&self) {}
    fn workspace_id(&self) -> Option<String> {
        None
    }
    fn set_workspace_id(&self, _id: &str) {}
}

fn publisher(clock: &Arc<ManualClock>) -> GrokbotPublisher {
    let store: Arc<dyn TextStore> = Arc::new(InMemoryStringStore::default());
    GrokbotPublisher::new(&store, Arc::clone(clock) as Arc<dyn Clock>)
}

fn manual_clock() -> Arc<ManualClock> {
    Arc::new(ManualClock::new(
        Utc.with_ymd_and_hms(2026, 8, 25, 15, 0, 0).unwrap(),
    ))
}

fn client_for(server: &MockServer) -> GrokbotUsageClient {
    let http = CursorHttp::with_base_url(
        ProviderErrorContext::Grokbot,
        &server.uri(),
        Some("test"),
        Arc::new(FixedClock::from_unix(1_787_000_000)),
    )
    .unwrap();
    GrokbotUsageClient::new(http).with_rpc_base_url(&server.uri())
}

async fn mount_ok(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/api/dashboard/get-sand-usage-status"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"usagePercent":42.5,"includedLimitZero":false,"hasNonZeroIncludedLimit":true,
                "hasAvailableUsage":true}"#,
        ))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path(
            "/aiserver.v1.DashboardService/GetGrokBotUsageBreakdown",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"weeklyLimitPpm":{"periodStartMs":"1794425100000","periodEndMs":"1795029900000",
            "bySource":[{"dayMs":"1794355200000","source":"GROK_BOT_USAGE_SOURCE_USER","ppm":100000}]}}"#,
        ))
        .mount(server)
        .await;
}

fn live_poller(
    server: &MockServer,
    session: &Arc<FakeSession>,
    clock: &Arc<ManualClock>,
) -> Arc<GrokbotUsagePoller> {
    GrokbotUsagePoller::new(GrokbotPollerConfig::live(
        session.clone(),
        publisher(clock),
        clock.clone(),
        client_for(server),
        Arc::new(|| true),
        Arc::new(|_| None),
    ))
}

/// The live config fetches the snapshot and the breakdown from the server and
/// publishes both.
#[tokio::test]
async fn test_live_config_publishes_snapshot_and_breakdown() {
    let server = MockServer::start().await;
    mount_ok(&server).await;
    let session = FakeSession::new();
    let clock = manual_clock();
    let poller = live_poller(&server, &session, &clock);
    assert_eq!(poller.perform_refresh().await, PollOutcome::Success);
    assert_eq!(poller.headline_used_percent(), Some(42.5));
    assert_eq!(poller.last_refreshed_at(), Some(clock.now()));
    assert_eq!(poller.alert_account().as_deref(), Some("a@b.com"));
    let breakdown = poller.breakdown().expect("breakdown fetched");
    assert_eq!(breakdown.slices.len(), 1);
    assert!((breakdown.slices[0].value - 10.0).abs() < 1e-9);
}

/// `refresh_now` on the trait runs one refresh through the polling loop.
#[tokio::test]
async fn test_refresh_now_runs_a_refresh_through_the_loop() {
    let server = MockServer::start().await;
    mount_ok(&server).await;
    let session = FakeSession::new();
    let clock = manual_clock();
    let poller = live_poller(&server, &session, &clock);
    poller.refresh_now().await;
    assert_eq!(poller.headline_used_percent(), Some(42.5));
    assert!(!poller.is_refreshing());
}

fn snapshot() -> GrokbotSnapshot {
    GrokbotSnapshot {
        fetched_at: Utc.with_ymd_and_hms(2026, 8, 25, 15, 0, 0).unwrap(),
        used_percent: 40.0,
        period_start: None,
        resets_at: None,
        entitlement: GrokbotEntitlement::Cursor,
        account_email: None,
        has_included_allowance: true,
    }
}

/// While a fetch is in flight the poller reports `is_refreshing`, and a
/// second refresh is skipped instead of running concurrently.
#[tokio::test]
async fn test_is_refreshing_is_true_only_while_a_fetch_is_in_flight() {
    let session = FakeSession::new();
    let clock = manual_clock();
    let cell: Arc<OnceLock<Arc<GrokbotUsagePoller>>> = Arc::new(OnceLock::new());
    let seen: Arc<Mutex<Vec<(bool, PollOutcome)>>> = Arc::new(Mutex::new(Vec::new()));
    let (cell2, seen2) = (Arc::clone(&cell), Arc::clone(&seen));
    let fetch: FetchFn = Arc::new(move |_cookie, _email| {
        let (cell, seen) = (Arc::clone(&cell2), Arc::clone(&seen2));
        Box::pin(async move {
            let poller = cell.get().unwrap();
            let nested = poller.perform_refresh().await;
            seen.lock().unwrap().push((poller.is_refreshing(), nested));
            Ok(snapshot())
        })
    });
    let poller = GrokbotUsagePoller::new(GrokbotPollerConfig {
        session: session.clone(),
        publisher: publisher(&clock),
        clock: clock.clone(),
        fetch,
        fetch_breakdown: None,
        is_enabled: Arc::new(|| true),
        poll_interval: Arc::new(|_| None),
    });
    cell.set(Arc::clone(&poller)).ok().unwrap();
    assert!(!poller.is_refreshing());
    assert_eq!(poller.perform_refresh().await, PollOutcome::Success);
    assert_eq!(*seen.lock().unwrap(), vec![(true, PollOutcome::Skipped)]);
    assert!(!poller.is_refreshing());
}

/// A breakdown fetch that returns nothing keeps the one already shown.
#[tokio::test]
async fn test_empty_breakdown_reply_keeps_the_previous_breakdown() {
    let session = FakeSession::new();
    let clock = manual_clock();
    let first = Arc::new(AtomicBool::new(true));
    let fetch: FetchFn = Arc::new(|_, _| Box::pin(async { Ok(snapshot()) }));
    let fetch_breakdown: BreakdownFetchFn = Arc::new(move |_| {
        let first = first.swap(false, Ordering::SeqCst);
        Box::pin(async move {
            Ok(first.then(|| GrokbotUsageBreakdown {
                unit: tokenmon_provider_grokbot::BreakdownUnit::WeeklyLimitPercent,
                slices: Vec::new(),
                period_start: None,
                period_end: None,
            }))
        })
    });
    let poller = GrokbotUsagePoller::new(GrokbotPollerConfig {
        session,
        publisher: publisher(&clock),
        clock,
        fetch,
        fetch_breakdown: Some(fetch_breakdown),
        is_enabled: Arc::new(|| true),
        poll_interval: Arc::new(|_| None),
    });
    poller.perform_refresh().await;
    let kept = poller.breakdown();
    assert!(kept.is_some());
    poller.perform_refresh().await;
    assert_eq!(poller.breakdown(), kept);
}

/// A failed snapshot fetch is the provider's last error and publishes no
/// breakdown.
#[tokio::test]
async fn test_failed_snapshot_reports_last_error_and_no_breakdown() {
    let session = FakeSession::new();
    let clock = manual_clock();
    let fetch: FetchFn = Arc::new(|_, _| {
        Box::pin(async {
            Err(ProviderError::Network(
                ProviderErrorContext::Grokbot,
                "down".to_owned(),
            ))
        })
    });
    let poller = GrokbotUsagePoller::new(GrokbotPollerConfig {
        session,
        publisher: publisher(&clock),
        clock,
        fetch,
        fetch_breakdown: None,
        is_enabled: Arc::new(|| true),
        poll_interval: Arc::new(|_| None),
    });
    assert!(matches!(
        poller.perform_refresh().await,
        PollOutcome::Failure { .. }
    ));
    assert!(matches!(
        poller.last_error(),
        Some(ProviderError::Network(..))
    ));
    assert_eq!(poller.breakdown(), None);
}

/// An auth event delivered after the poller was dropped is ignored.
#[tokio::test]
async fn test_auth_event_after_poller_dropped_is_ignored() {
    let server = MockServer::start().await;
    let session = FakeSession::new();
    let clock = manual_clock();
    let poller = live_poller(&server, &session, &clock);
    drop(poller);
    assert_eq!(session.listeners.lock().unwrap().len(), 1);
    session.fire(AuthEvent::AccountReset);
}
