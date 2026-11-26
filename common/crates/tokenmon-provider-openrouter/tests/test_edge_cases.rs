//! Edge cases of the OpenRouter crate: non-finite money, malformed activity
//! dates, unknown limit windows, estimator output, key redaction and the
//! poller's skip, factory-failure and clearing paths.
#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use tokenmon_core::polling::PollOutcome;
use tokenmon_core::usage::{Calendar, WeekOffset};
use tokenmon_core::{
    BoxFuture, InMemoryCredentialStore, ManualClock, ProviderError, ProviderErrorContext,
    ProviderUsagePoller,
};
use tokenmon_provider_openrouter::{
    ActivityRow, ApiKey, BudgetSource, CostEstimator, CreditsData, Endpoint, EndpointFetcher,
    KeyData, LimitReset, NoCostEstimator, OpenRouterAuthSession, OpenRouterDailyChart,
    OpenRouterSnapshot, OpenRouterUsageClient, OpenRouterUsagePoller, models_from_rows,
    parse_activity,
};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, 12, 0, 0).unwrap()
}

fn key(usage: f64) -> KeyData {
    KeyData {
        label: None,
        usage,
        usage_daily: None,
        usage_weekly: None,
        usage_monthly: None,
        limit: None,
        limit_remaining: None,
        limit_reset: None,
        is_free_tier: None,
        is_management_key: None,
    }
}

fn row(date: &str, usage: f64) -> ActivityRow {
    ActivityRow {
        date: date.to_owned(),
        model: "vendor/model".to_owned(),
        provider_name: None,
        usage,
        requests: 1,
        prompt_tokens: 10,
        completion_tokens: 5,
        reasoning_tokens: 0,
    }
}

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
}

/// A NaN all-time spend never reaches the displayed spend.
#[test]
fn test_nan_key_usage_is_shown_as_zero() {
    let s = OpenRouterSnapshot::build(&key(f64::NAN), None, None, now(), &NoCostEstimator);
    assert_eq!(s.used_usd, 0.0);
    assert!(s.used_percent().is_none());
}

/// Infinite credits and spend give a defined percent (never NaN).
#[test]
fn test_infinite_credits_give_a_finite_percent() {
    let credits = CreditsData {
        total_credits: f64::INFINITY,
        total_usage: f64::INFINITY,
    };
    let s = OpenRouterSnapshot::build(&key(1.0), Some(credits), None, now(), &NoCostEstimator);
    let pct = s.used_percent().unwrap();
    assert!((0.0..=100.0).contains(&pct), "{pct}");
}

/// Only well-formed `yyyy-MM-dd` activity dates count towards a day; today's
/// `usage_daily` fills in only when activity left today out.
#[test]
fn test_activity_dates_must_be_well_formed_and_today_prefers_activity() {
    let rows = [
        row("2026-10-12 00:00:00", 1.0),
        row("2026-10-12", 0.5),
        row("26-10-1234", 9.0),
        row("2026-1-123", 9.0),
        row("2026-1x-12", 9.0),
        row("2026-13-45", 9.0),
        row("short", 9.0),
        row("2026-10-14", 2.0),
    ];
    let mut k = key(10.0);
    k.usage_daily = Some(7.0);
    let s = OpenRouterSnapshot::build(&k, None, Some(&rows), now(), &NoCostEstimator);
    assert_eq!(s.spend_by_utc_day.len(), 2);
    assert!((s.spend_by_utc_day[&day(12)] - 1.5).abs() < 1e-9);
    assert!(
        (s.spend_by_utc_day[&day(14)] - 2.0).abs() < 1e-9,
        "activity wins over usage_daily"
    );

    let without_today =
        OpenRouterSnapshot::build(&k, None, Some(&rows[..2]), now(), &NoCostEstimator);
    assert!((without_today.spend_by_utc_day[&day(14)] - 7.0).abs() < 1e-9);
}

/// An unrecognised limit window falls back to the key's all-time spend and
/// has no reset instant.
#[test]
fn test_unknown_limit_window_falls_back_to_total_spend() {
    let mut k = key(30.0);
    k.limit = Some(100.0);
    k.limit_reset = Some("hourly".to_owned());
    k.usage_daily = Some(1.0);
    let s = OpenRouterSnapshot::build(&k, None, None, now(), &NoCostEstimator);
    assert_eq!(s.budget_source, Some(BudgetSource::KeyLimit));
    assert_eq!(s.limit_reset, Some(LimitReset::Other));
    assert_eq!(s.used_usd, 30.0);
    assert_eq!(s.remaining_usd, Some(70.0));
    assert!(s.resets_at().is_none());
    assert_eq!(BudgetSource::AccountCredits.label(), "Account credits");
}

struct Fixed(Option<f64>);

impl CostEstimator for Fixed {
    fn estimate(&self, _: &str, _: i64, _: i64) -> Option<f64> {
        self.0
    }
}

/// Estimates that are not finite and positive are ignored; a sane one marks
/// the model's cost as estimated.
#[test]
fn test_estimator_output_must_be_finite_and_positive() {
    let rows = [row("2026-10-14", 0.0)];
    for bad in [
        Some(f64::NAN),
        Some(f64::INFINITY),
        Some(-1.0),
        Some(0.0),
        None,
    ] {
        let m = models_from_rows(&rows, &Fixed(bad));
        assert!(!m[0].is_cost_estimated, "{bad:?}");
        assert_eq!(m[0].cost_usd, 0.0);
    }
    let m = models_from_rows(&rows, &Fixed(Some(0.25)));
    assert!(m[0].is_cost_estimated);
    assert_eq!(m[0].cost_usd, 0.25);
}

/// Token counts must be integers: a fractional count rejects the payload.
#[test]
fn test_activity_rejects_fractional_token_counts_and_accepts_integral_floats() {
    let body = |n: &str| {
        format!(
            r#"{{"data":[{{"date":"2026-10-14","model":"a/b","usage":1,"requests":{n},"prompt_tokens":1,"completion_tokens":1,"reasoning_tokens":0}}]}}"#
        )
    };
    assert!(parse_activity(body("1.5").as_bytes()).is_err());
    assert_eq!(
        parse_activity(body("2.0").as_bytes()).unwrap()[0].requests,
        2
    );
}

/// Redacting with an empty key leaves the text untouched; otherwise every
/// occurrence is replaced.
#[test]
fn test_api_key_redaction() {
    assert_eq!(ApiKey::new("").redact("text"), "text");
    assert_eq!(
        ApiKey::new("sk-or-v1-s").redact("a sk-or-v1-s b sk-or-v1-s"),
        "a [redacted] b [redacted]"
    );
}

struct NoopFetcher;

impl EndpointFetcher for NoopFetcher {
    fn fetch(&self, _: Endpoint) -> BoxFuture<'_, Result<Vec<u8>, ProviderError>> {
        Box::pin(async { Ok(br#"{"data":{"usage":1,"is_management_key":false}}"#.to_vec()) })
    }
}

/// The client's debug output names the type without exposing its internals.
#[test]
fn test_client_debug_is_opaque() {
    let c = OpenRouterUsageClient::new(Arc::new(NoopFetcher), Arc::new(NoCostEstimator));
    assert_eq!(format!("{c:?}"), "OpenRouterUsageClient { .. }");
}

fn poller_with_factory(auth: &Arc<OpenRouterAuthSession>, ok: bool) -> Arc<OpenRouterUsagePoller> {
    let factory = Arc::new(move |_: &ApiKey| {
        if ok {
            Ok(OpenRouterUsageClient::new(
                Arc::new(NoopFetcher),
                Arc::new(NoCostEstimator),
            ))
        } else {
            Err(ProviderError::Network(
                ProviderErrorContext::OpenRouter,
                "no client".to_owned(),
            ))
        }
    });
    OpenRouterUsagePoller::new(
        auth.clone(),
        Arc::new(ManualClock::from_unix(1_790_000_000)),
        factory,
        Arc::new(|| true),
        Arc::new(|open| open.then_some(Duration::from_secs(60))),
    )
}

fn auth() -> Arc<OpenRouterAuthSession> {
    Arc::new(OpenRouterAuthSession::new(Arc::new(
        InMemoryCredentialStore::default(),
    )))
}

/// A factory that cannot build a client fails the refresh and surfaces its
/// error.
#[tokio::test]
async fn test_factory_failure_fails_the_refresh_with_its_error() {
    let a = auth();
    a.save_api_key("sk-or-v1-abc").unwrap();
    let p = poller_with_factory(&a, false);
    assert_eq!(
        p.perform_refresh().await,
        PollOutcome::Failure { retry_after: None }
    );
    assert!(matches!(p.last_error(), Some(ProviderError::Network(..))));
    assert!(p.snapshot().is_none());
}

/// A key the session already flagged as needing sign-in is not fetched.
#[tokio::test]
async fn test_flagged_session_is_skipped_without_fetching() {
    let a = auth();
    a.save_api_key("sk-or-v1-abc").unwrap();
    a.set_needs_sign_in(true);
    let p = poller_with_factory(&a, true);
    assert_eq!(p.perform_refresh().await, PollOutcome::Skipped);
    assert!(p.snapshot().is_none());
}

/// Menu state toggles, a refresh publishes and `clear_snapshot` drops the
/// data again.
#[tokio::test]
async fn test_menu_state_and_clear_snapshot() {
    let a = auth();
    a.save_api_key("sk-or-v1-abc").unwrap();
    let p = poller_with_factory(&a, true);
    p.set_menu_open(true);
    p.set_menu_open(true);
    assert!(p.is_menu_open());
    p.set_menu_open(false);
    assert!(!p.is_menu_open());
    assert_eq!(p.perform_refresh().await, PollOutcome::Success);
    assert!(p.snapshot().is_some() && p.last_refreshed_at().is_some());
    p.clear_snapshot();
    assert!(p.snapshot().is_none() && p.last_refreshed_at().is_none());
}

/// A budget and spend that are both infinite produce zero percent rather
/// than NaN.
#[test]
fn test_indeterminate_budget_ratio_is_reported_as_zero_percent() {
    let mut s = OpenRouterSnapshot::build(&key(1.0), None, None, now(), &NoCostEstimator);
    s.budget_usd = Some(f64::INFINITY);
    s.used_usd = f64::INFINITY;
    assert_eq!(s.used_percent(), Some(0.0));
}

/// At the end of the representable calendar no window end exists, so weekly
/// and monthly charts show no bars instead of panicking.
#[test]
fn test_charts_at_the_end_of_time_have_no_bars() {
    for reset in ["weekly", "monthly"] {
        let mut k = key(10.0);
        k.limit = Some(100.0);
        k.limit_reset = Some(reset.to_owned());
        let snap = OpenRouterSnapshot::build(&k, None, None, now(), &NoCostEstimator);
        let end = DateTime::<Utc>::MAX_UTC;
        let chart = OpenRouterDailyChart::make(&snap, end).expect("budget exists");
        assert!(
            chart.days(WeekOffset::CURRENT, &Calendar::utc()).is_empty(),
            "{reset}"
        );
    }
}
