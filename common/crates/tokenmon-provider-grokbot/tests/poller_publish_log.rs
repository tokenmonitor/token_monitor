//! The refresh log line. Lives in its own test binary because it installs a
//! thread-local `tracing` subscriber, which must not race with callsites
//! registered by unrelated tests.

#![allow(clippy::unwrap_used, clippy::assert_is_empty)]

use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use tokenmon_core::storage::{InMemoryStringStore, TextStore};
use tokenmon_core::{
    AuthEventListener, AuthSession, AuthSessionState, CapturedSession, Clock, CredentialStoreError,
    ManualClock, ProviderAuthSession, SessionGeneration,
};
use tokenmon_provider_grokbot::{
    FetchFn, GrokbotEntitlement, GrokbotPollerConfig, GrokbotPublisher, GrokbotSnapshot,
    GrokbotUsagePoller,
};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

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

#[derive(Default)]
struct Messages(Mutex<Vec<String>>);

struct Capture(Arc<Messages>);

struct MessageVisitor(String);

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

impl Subscriber for Capture {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        self.0.0.lock().unwrap().push(visitor.0);
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

/// A published refresh logs used and remaining percent.
#[tokio::test]
async fn test_publish_logs_used_and_remaining_percent() {
    let messages = Arc::new(Messages::default());
    let _guard = tracing::subscriber::set_default(Capture(Arc::clone(&messages)));
    let session = FakeSession::new();
    let clock = manual_clock();
    let fetch: FetchFn = Arc::new(|_, _| Box::pin(async { Ok(snapshot()) }));
    let poller = GrokbotUsagePoller::new(GrokbotPollerConfig {
        session,
        publisher: publisher(&clock),
        clock,
        fetch,
        fetch_breakdown: None,
        is_enabled: Arc::new(|| true),
        poll_interval: Arc::new(|_| None),
    });
    poller.perform_refresh().await;
    let logged = messages.0.lock().unwrap().clone();
    assert!(
        logged
            .iter()
            .any(|m| m.contains("Grokbot refresh: 40.0% used (60% left)")),
        "logged: {logged:?}"
    );
}
