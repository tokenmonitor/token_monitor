//! Small public surfaces of the Grokbot crate: source labels and colors, the
//! production client constructor, fetcher snapshot memory and date-key
//! handling.

#![allow(clippy::unwrap_used, clippy::assert_is_empty)]

use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use tokenmon_core::storage::{InMemoryStringStore, TextStore};
use tokenmon_core::{
    AuthEventListener, AuthSession, AuthSessionState, Clock, FixedClock, ManualClock,
    ProviderErrorContext, SessionGeneration, UsageFetcher,
};
use tokenmon_provider_cursor::CursorHttp;
use tokenmon_provider_grokbot::{
    GrokbotFetcher, GrokbotPublisher, GrokbotUsageClient, GrokbotUsageSource, parse_usage_status,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ALL: [GrokbotUsageSource; 5] = [
    GrokbotUsageSource::User,
    GrokbotUsageSource::Automation,
    GrokbotUsageSource::Agent,
    GrokbotUsageSource::Group,
    GrokbotUsageSource::Other,
];

/// Every source has a distinct raw name, a distinct label and a distinct
/// color, and the raw name round-trips through the wire parser.
#[test]
fn test_every_source_has_distinct_name_label_and_color() {
    let raws: Vec<&str> = ALL.iter().map(|s| s.raw_value()).collect();
    assert_eq!(raws, ["user", "automation", "agent", "group", "other"]);
    let labels: Vec<&str> = ALL.iter().map(|s| s.display_name()).collect();
    assert_eq!(
        labels,
        ["You", "Routines", "Bot to Bot", "Group chats", "Other"]
    );
    for (i, a) in ALL.iter().enumerate() {
        assert_eq!(GrokbotUsageSource::from_wire(a.raw_value()), *a);
        for b in &ALL[i + 1..] {
            assert_ne!(a.color(), b.color(), "{a:?} and {b:?} share a color");
        }
    }
}

/// Protobuf names and numbers map to sources; unknown values are `Other`.
#[test]
fn test_from_wire_accepts_enum_names_and_numbers() {
    assert_eq!(
        GrokbotUsageSource::from_wire("GROK_BOT_USAGE_SOURCE_GROUP"),
        GrokbotUsageSource::Group
    );
    assert_eq!(
        GrokbotUsageSource::from_wire("3"),
        GrokbotUsageSource::Agent
    );
    assert_eq!(
        GrokbotUsageSource::from_wire("mystery"),
        GrokbotUsageSource::Other
    );
}

/// The production constructor targets the real host without any I/O.
#[test]
fn test_production_client_builds_without_network() {
    let client = GrokbotUsageClient::production(Some("1.2.3"), Arc::new(FixedClock::from_unix(0)));
    assert!(client.is_ok());
}

struct Session(Mutex<AuthSessionState>);

impl AuthSession for Session {
    fn cookie_header(&self) -> Option<String> {
        Some("WorkosCursorSessionToken=x".to_owned())
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

/// A successful fetch is remembered by the fetcher, and `clear_snapshot`
/// forgets it (sign-out).
#[tokio::test]
async fn test_fetcher_remembers_last_snapshot_until_cleared() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/dashboard/get-sand-usage-status"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"usagePercent":12}"#))
        .mount(&server)
        .await;
    let http = CursorHttp::with_base_url(
        ProviderErrorContext::Grokbot,
        &server.uri(),
        Some("test"),
        Arc::new(FixedClock::from_unix(1_787_000_000)),
    )
    .unwrap();
    let mut state = AuthSessionState::default();
    state.mark_signed_in(Some("a@b.com".into()));
    let fetcher = GrokbotFetcher::new(
        GrokbotUsageClient::new(http),
        Arc::new(Session(Mutex::new(state))),
        Arc::new(FixedClock::from_unix(1_787_000_000)),
    );
    assert_eq!(fetcher.snapshot(), None);
    let fetched = fetcher.fetch().await.unwrap();
    assert_eq!(fetcher.snapshot(), Some(fetched));
    fetcher.clear_snapshot();
    assert_eq!(fetcher.snapshot(), None);
}

/// The publisher exposes its hourly activity store, which starts empty.
#[test]
fn test_publisher_exposes_hourly_store() {
    let clock = Arc::new(ManualClock::new(
        Utc.with_ymd_and_hms(2026, 8, 25, 15, 0, 0).unwrap(),
    ));
    let store: Arc<dyn TextStore> = Arc::new(InMemoryStringStore::default());
    let publisher = GrokbotPublisher::new(&store, Arc::clone(&clock) as Arc<dyn Clock>);
    let hourly = publisher.hourly();
    assert_eq!(hourly.last_used_percent(), None);
    assert!(hourly.hour_weights().iter().all(|w| *w == 0.0));
}

/// A reset key holding a number (neither a string nor an object) is ignored
/// rather than misread as a date.
#[test]
fn test_numeric_reset_value_is_not_a_date() {
    let now = Utc.with_ymd_and_hms(2026, 8, 25, 15, 0, 0).unwrap();
    let s = parse_usage_status(
        br#"{"usagePercent":5,"nextResetTimestampUtc":1787000000}"#,
        None,
        now,
    )
    .unwrap();
    assert_eq!(s.resets_at, None);
}
