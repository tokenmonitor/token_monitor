//! Ports of the `PollerSessionGuardTests` Grokbot usage breakdown tests: the
//! breakdown is fetched after a successful snapshot, never strikes the shared
//! Cursor session, and is dropped when the session changed.

#![allow(clippy::unwrap_used, clippy::assert_is_empty)]

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeZone, Utc};
use tokenmon_core::polling::PollOutcome;
use tokenmon_core::storage::{InMemoryStringStore, TextStore};
use tokenmon_core::{
    AuthEvent, AuthEventListener, AuthSession, AuthSessionState, CapturedSession,
    CredentialStoreError, ManualClock, ProviderAuthSession, ProviderError, ProviderErrorContext,
    ProviderUsagePoller, SessionGeneration,
};
use tokenmon_provider_grokbot::{
    BreakdownFetchFn, BreakdownSlice, BreakdownUnit, FetchFn, GrokbotEntitlement,
    GrokbotPollerConfig, GrokbotPublisher, GrokbotSnapshot, GrokbotUsageBreakdown,
    GrokbotUsagePoller, GrokbotUsageSource,
};

const CTX: ProviderErrorContext = ProviderErrorContext::Grokbot;

fn at(h: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 25, h, 0, 0).unwrap()
}

/// A Cursor session fake over the real state machine that fires events.
struct FakeSession {
    state: Mutex<AuthSessionState>,
    listeners: Mutex<Vec<Arc<dyn AuthEventListener>>>,
    cookie: bool,
}

impl FakeSession {
    fn new(cookie: bool) -> Arc<Self> {
        let session = Arc::new(Self {
            state: Mutex::new(AuthSessionState::default()),
            listeners: Mutex::new(Vec::new()),
            cookie,
        });
        session
            .state
            .lock()
            .unwrap()
            .mark_signed_in(Some("a@b.com".into()));
        session
    }

    fn fire(&self, event: AuthEvent) {
        let listeners = self.listeners.lock().unwrap().clone();
        for listener in listeners {
            listener.on_auth_event(event);
        }
    }

    fn st(&self) -> AuthSessionState {
        self.state.lock().unwrap().clone()
    }
}

impl AuthSession for FakeSession {
    fn cookie_header(&self) -> Option<String> {
        self.cookie.then(|| "WorkosCursorSessionToken=x".to_owned())
    }
    fn state(&self) -> AuthSessionState {
        self.st()
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

fn snapshot(used: f64) -> GrokbotSnapshot {
    GrokbotSnapshot {
        fetched_at: at(15),
        used_percent: used,
        period_start: Some(Utc.with_ymd_and_hms(2026, 8, 20, 11, 0, 0).unwrap()),
        resets_at: Some(Utc.with_ymd_and_hms(2026, 8, 27, 11, 0, 0).unwrap()),
        entitlement: GrokbotEntitlement::Cursor,
        account_email: None,
        has_included_allowance: true,
    }
}

fn breakdown(user: f64) -> GrokbotUsageBreakdown {
    GrokbotUsageBreakdown {
        unit: BreakdownUnit::WeeklyLimitPercent,
        period_start: None,
        period_end: None,
        slices: vec![BreakdownSlice {
            source: GrokbotUsageSource::User,
            value: user,
        }],
    }
}

/// A poller whose snapshot fetch is `snap` and whose breakdown fetch is `crumbs`.
fn poller(
    session: &Arc<FakeSession>,
    snap: impl Fn() -> Result<GrokbotSnapshot, ProviderError> + Send + Sync + 'static,
    crumbs: impl Fn() -> Result<Option<GrokbotUsageBreakdown>, ProviderError> + Send + Sync + 'static,
) -> Arc<GrokbotUsagePoller> {
    let clock = Arc::new(ManualClock::new(at(15)));
    let store: Arc<dyn TextStore> = Arc::new(InMemoryStringStore::default());
    let fetch: FetchFn = Arc::new(move |_cookie, _email| {
        let result = snap();
        Box::pin(async move { result })
    });
    let fetch_breakdown: BreakdownFetchFn = Arc::new(move |_cookie| {
        let result = crumbs();
        Box::pin(async move { result })
    });
    GrokbotUsagePoller::new(GrokbotPollerConfig {
        session: session.clone(),
        publisher: GrokbotPublisher::new(&store, clock.clone()),
        clock,
        fetch,
        fetch_breakdown: Some(fetch_breakdown),
        is_enabled: Arc::new(|| true),
        poll_interval: Arc::new(|_| None),
    })
}

/// The breakdown is published after a successful snapshot.
#[tokio::test]
async fn test_grokbot_publishes_breakdown_after_snapshot() {
    let session = FakeSession::new(true);
    let p = poller(
        &session,
        || Ok(snapshot(40.0)),
        || Ok(Some(breakdown(25.0))),
    );
    assert_eq!(p.perform_refresh().await, PollOutcome::Success);
    assert_eq!(p.breakdown(), Some(breakdown(25.0)));
}

/// The breakdown only adds detail: its failure keeps the last one, shows no
/// error, and never counts against the shared Cursor session.
#[tokio::test]
async fn test_grokbot_breakdown_failure_keeps_last_and_session() {
    let session = FakeSession::new(true);
    let fails = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&fails);
    let p = poller(
        &session,
        || Ok(snapshot(40.0)),
        move || {
            if flag.load(Ordering::SeqCst) {
                Err(ProviderError::Unauthorized(CTX))
            } else {
                Ok(Some(breakdown(25.0)))
            }
        },
    );
    p.perform_refresh().await;
    fails.store(true, Ordering::SeqCst);
    for _ in 0..3 {
        p.perform_refresh().await;
    }
    assert_eq!(p.breakdown(), Some(breakdown(25.0)));
    assert!(p.last_error().is_none());
    let state = session.st();
    assert!(state.is_signed_in && !state.needs_sign_in);
    assert_eq!(state.consecutive_auth_failures, 0);
}

/// A failed snapshot never reaches for the breakdown.
#[tokio::test]
async fn test_grokbot_breakdown_skipped_when_snapshot_fails() {
    let session = FakeSession::new(true);
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let p = poller(
        &session,
        || Err(ProviderError::Network(CTX, "offline".into())),
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Some(breakdown(25.0)))
        },
    );
    p.perform_refresh().await;
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(p.breakdown().is_none());
}

/// A breakdown that arrives after the session was replaced is dropped, and
/// sign-out clears the one on screen.
#[tokio::test]
async fn test_grokbot_stale_breakdown_is_dropped_and_sign_out_clears_it() {
    let session = FakeSession::new(true);
    let switch = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&switch);
    let swapper = Arc::clone(&session);
    let p = poller(
        &session,
        || Ok(snapshot(40.0)),
        move || {
            if flag.load(Ordering::SeqCst) {
                // Sign-out and sign-in while the request is in flight.
                swapper
                    .state
                    .lock()
                    .unwrap()
                    .mark_signed_in(Some("new@b.com".into()));
            }
            Ok(Some(breakdown(25.0)))
        },
    );
    p.perform_refresh().await;
    assert!(p.breakdown().is_some());
    session.fire(AuthEvent::SignedOut);
    assert!(p.breakdown().is_none());

    switch.store(true, Ordering::SeqCst);
    p.perform_refresh().await;
    assert!(p.breakdown().is_none());
}
