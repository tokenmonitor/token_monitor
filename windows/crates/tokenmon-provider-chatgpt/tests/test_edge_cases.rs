//! Edge cases across the ChatGPT crate: cookie and page classification, JWT
//! claim shapes, saved-reset counts, epoch conversion, the poller's trait
//! surface and the usage client's token-cache paths.
#![allow(
    clippy::unwrap_used,
    clippy::unused_async,
    clippy::unused_async_trait_impl,
    clippy::option_option
)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, TimeZone, Utc};
use common::{FakeSession, FakeTransport, USAGE_BODY, jwt, jwt_exp};
use tokenmon_core::net::ProviderHttp;
use tokenmon_core::polling::PollOutcome;
use tokenmon_core::storage::{DailyQuotaDeltaStore, InMemoryStringStore};
use tokenmon_core::{
    AuthEvent, Clock, ManualClock, ProviderError, ProviderErrorContext as C, ProviderUsagePoller,
    SystemClock,
};
use tokenmon_provider_chatgpt::jwt::account_id;
use tokenmon_provider_chatgpt::time::{epoch_secs, from_epoch_secs};
use tokenmon_provider_chatgpt::{
    AccessTokenCache, ChatGptPoller, ChatGptPollerConfig, ChatGptUsageClient, Fetch, FetchFn,
    HttpTransport, RawReply, UsageWindow, is_return_page, looks_like_auth_cookie, parse_usage,
};

fn at(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

const NOW: i64 = 1_790_000_000;
const COOKIE: &str = "__Secure-next-auth.session-token=live";

/// Only session-token cookies count as auth cookies, case-insensitively, and
/// CSRF cookies never do.
#[test]
fn test_looks_like_auth_cookie_accepts_session_tokens_and_rejects_csrf() {
    assert!(looks_like_auth_cookie("__Secure-next-auth.session-token"));
    assert!(looks_like_auth_cookie("My_Session_Token"));
    assert!(!looks_like_auth_cookie(
        "__Host-next-auth.csrf-session-token"
    ));
    assert!(!looks_like_auth_cookie("session"));
    assert!(!looks_like_auth_cookie("_ga"));
}

/// A chatgpt.com page counts as the return page unless the path mentions
/// login, signin or an `/auth` segment.
#[test]
fn test_is_return_page_rejects_each_auth_path_marker() {
    assert!(is_return_page("app.chatgpt.com", "/c/1"));
    assert!(!is_return_page("chatgpt.com", "/Login"));
    assert!(!is_return_page("chatgpt.com", "/signin/x"));
    assert!(!is_return_page("chatgpt.com", "/x/auth/callback"));
    assert!(!is_return_page("chatgpt.com.evil.example", "/"));
}

/// An auth claim that is not an object (or the wrong claim) yields no account id,
/// whatever JSON type it has.
#[test]
fn test_account_id_ignores_non_object_auth_claims() {
    for value in [
        "true",
        "7",
        "-3",
        "1.5",
        r#""text""#,
        "null",
        "[1,[2]]",
        "[]",
    ] {
        let token = jwt(&format!(
            r#"{{"other":{{"a":1}},"https://api.openai.com/auth":{value}}}"#
        ));
        assert_eq!(account_id(&token), None, "claim {value}");
    }
}

/// Unrelated members, in any order and of any depth, are skipped without
/// disturbing the account id.
#[test]
fn test_account_id_skips_unrelated_members() {
    let token = jwt(
        r#"{"x":[1,{"y":2}],"https://api.openai.com/auth":{"chatgpt_account_id":"acct_1"},"z":{"k":"v"}}"#,
    );
    assert_eq!(account_id(&token).as_deref(), Some("acct_1"));
}

/// Saved-reset counts that are fractional, negative, huge or non-numeric are
/// dropped; a whole count is kept.
#[test]
fn test_saved_resets_rejects_implausible_counts() {
    let body = |count: &str| {
        format!(r#"{{"rate_limit_reset_credits":{{"available_count":{count}}},"rate_limit":{{}}}}"#)
    };
    assert!(
        parse_usage(body("1.5").as_bytes())
            .unwrap()
            .saved_resets
            .is_none()
    );
    assert!(
        parse_usage(body("-1").as_bytes())
            .unwrap()
            .saved_resets
            .is_none()
    );
    assert!(
        parse_usage(body("1e12").as_bytes())
            .unwrap()
            .saved_resets
            .is_none()
    );
    assert!(
        parse_usage(body(r#""two""#).as_bytes())
            .unwrap()
            .saved_resets
            .is_none()
    );
    assert!(
        parse_usage(body("0").as_bytes())
            .unwrap()
            .saved_resets
            .is_none()
    );
    assert!(
        parse_usage(body("3").as_bytes())
            .unwrap()
            .saved_resets
            .is_some()
    );
}

/// Remaining percent is the complement of used percent.
#[test]
fn test_remaining_percent_is_the_complement_of_used() {
    let w = UsageWindow {
        used_percent: 12.5,
        resets_at: None,
        window_seconds: None,
    };
    assert!((w.remaining_percent() - 87.5).abs() < 1e-9);
}

/// Epoch conversion rejects non-finite and implausible values and round
/// trips a fractional instant.
#[test]
fn test_epoch_conversion_is_total() {
    assert!(from_epoch_secs(f64::INFINITY).is_none());
    assert!(from_epoch_secs(f64::NEG_INFINITY).is_none());
    assert!(from_epoch_secs(f64::NAN).is_none());
    assert!(from_epoch_secs(9.0e12).is_none());
    let t = from_epoch_secs(1_787_300_000.5).unwrap();
    assert!((epoch_secs(t) - 1_787_300_000.5).abs() < 1e-3);
}

fn fetch_ok(body: &str) -> Fetch {
    Fetch {
        response: parse_usage(body.as_bytes()).unwrap(),
        fetched_at: at(NOW),
        set_cookie_headers: vec!["secret-cookie=1".to_owned()],
    }
}

fn fetch_fn(body: &'static str, calls: Arc<AtomicUsize>) -> Arc<FetchFn> {
    Arc::new(move |_cookie, _gen| {
        calls.fetch_add(1, Ordering::SeqCst);
        let r = Ok(fetch_ok(body));
        Box::pin(async move { r })
    })
}

fn poller_with(session: &Arc<FakeSession>, fetch: Arc<FetchFn>) -> Arc<ChatGptPoller> {
    let clock: Arc<dyn Clock> = Arc::new(ManualClock::from_unix(NOW));
    ChatGptPoller::new(ChatGptPollerConfig {
        session: session.clone(),
        clock: Arc::clone(&clock),
        token_cache: Arc::new(AccessTokenCache::new()),
        fetch,
        is_enabled: Arc::new(|| true),
        poll_interval: Arc::new(|open| open.then(|| std::time::Duration::from_secs(3600))),
        daily: DailyQuotaDeltaStore::new(
            Arc::new(InMemoryStringStore::default()),
            "chatgpt_daily_usage",
            clock,
        ),
    })
}

/// The poller's trait surface reports the menu state, the account email and a
/// manual refresh through the polling loop; the refresh flag is idle between
/// refreshes.
#[tokio::test]
async fn test_poller_trait_surface_reports_menu_account_and_refresh() {
    let calls = Arc::new(AtomicUsize::new(0));
    let session = FakeSession::signed_in_as(COOKIE, "me@example.com");
    let p = poller_with(&session, fetch_fn(USAGE_BODY, Arc::clone(&calls)));
    assert!(!p.is_menu_open());
    assert!(!p.is_refreshing());
    assert_eq!(p.alert_account().as_deref(), Some("me@example.com"));
    p.set_menu_open(true);
    p.set_menu_open(true);
    assert!(p.is_menu_open());
    p.set_menu_open(false);
    assert!(!p.is_menu_open());
    ProviderUsagePoller::refresh_now(&*p).await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(p.headline_used_percent().is_some());
    p.stop();
}

/// A weekly window without a reset instant still records usage, and the
/// poller reports no weekly reset it never saw.
#[tokio::test]
async fn test_weekly_window_without_reset_instant_is_recorded() {
    const BODY: &str = r#"{"rate_limit":{"primary_window":{"used_percent":5,"limit_window_seconds":18000},
        "secondary_window":{"used_percent":40,"limit_window_seconds":604800}}}"#;
    let calls = Arc::new(AtomicUsize::new(0));
    let session = FakeSession::signed_in(COOKIE);
    let p = poller_with(&session, fetch_fn(BODY, calls));
    assert!(matches!(p.perform_refresh().await, PollOutcome::Success));
    assert!(p.daily_budget_days().is_some());
    assert!(p.weekly_resets_at(at(NOW)).is_none());
    assert!(p.last_error().is_none());
}

/// A session event arriving after the poller was dropped is ignored.
#[tokio::test]
async fn test_session_event_after_the_poller_is_dropped_is_harmless() {
    let session = FakeSession::signed_in(COOKIE);
    let p = poller_with(
        &session,
        fetch_fn(USAGE_BODY, Arc::new(AtomicUsize::new(0))),
    );
    drop(p);
    session.fire(AuthEvent::AccountReset);
}

/// Without a token cache every fetch exchanges the cookie again.
#[tokio::test]
async fn test_client_without_a_cache_exchanges_every_time() {
    let transport = Arc::new(FakeTransport::new(jwt_exp(NOW + 3600)));
    let c = ChatGptUsageClient::new("c", Arc::clone(&transport), None, "1");
    c.fetch_usage(at(NOW)).await.unwrap();
    c.fetch_usage(at(NOW + 1)).await.unwrap();
    assert_eq!(transport.exchanges(), 2);
}

/// A cached token that fails with a non-auth error propagates the error
/// without a second exchange; an unauthorized one re-exchanges once.
#[tokio::test]
async fn test_cached_token_errors_propagate_or_re_exchange() {
    let cache = Arc::new(AccessTokenCache::new());
    let transport = Arc::new(FakeTransport::scripted(jwt_exp(NOW + 3600), |n| match n {
        2 => Err(ProviderError::Unauthorized(C::ChatGpt)),
        n if n <= 3 => Ok(RawReply {
            body: USAGE_BODY.as_bytes().to_vec(),
            set_cookies: vec![],
        }),
        _ => Err(ProviderError::BadResponse(C::ChatGpt, "boom".to_owned())),
    }));
    let c = ChatGptUsageClient::new("c", Arc::clone(&transport), Some(cache), "1");
    c.fetch_usage(at(NOW)).await.unwrap();
    c.fetch_usage(at(NOW + 5)).await.unwrap();
    assert_eq!(
        transport.exchanges(),
        2,
        "401 on the cached token re-exchanges"
    );
    let err = c.fetch_usage(at(NOW + 10)).await.unwrap_err();
    assert!(matches!(err, ProviderError::BadResponse(..)));
    assert_eq!(
        transport.exchanges(),
        2,
        "a non-auth error does not exchange"
    );
}

/// Debug output of a fetch never prints cookie values.
#[test]
fn test_fetch_debug_hides_cookie_values() {
    let shown = format!("{:?}", fetch_ok(USAGE_BODY));
    assert!(shown.contains("set_cookie_headers: 1"));
    assert!(!shown.contains("secret-cookie"));
}

/// The default-origin transport can be built (no request is made).
#[test]
fn test_http_transport_new_builds_without_network() {
    let http = ProviderHttp::new(C::ChatGpt, Some("test"), Arc::new(SystemClock)).unwrap();
    let t = HttpTransport::new(http);
    assert!(format!("{t:?}").contains("chatgpt.com"));
}
