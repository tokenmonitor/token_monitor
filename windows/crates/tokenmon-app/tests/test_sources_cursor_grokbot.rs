//! The Cursor and Grokbot typed sources (and their hourly sources) over
//! pollers fed by scripted fetches.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use common::{Env, env, query, t0};
use tokenmon_app::sources::{
    SnapshotView, TodayPart, cursor_hourly, cursor_source, grokbot_hourly, grokbot_source,
};
use tokenmon_core::settings::Settings;
use tokenmon_core::storage::{DailyQuotaDeltaStore, InMemoryStringStore, TextStore};
use tokenmon_core::usage::Calendar;
use tokenmon_core::{ProviderId, ProviderUsagePoller};
use tokenmon_provider_cursor::{
    CursorDayHourlyUsage, CursorFetchResult, CursorPollerConfig, CursorPoolUsage, CursorSnapshot,
    CursorUsagePoller, FetchFn as CursorFetch, PoolKind,
};
use tokenmon_provider_grokbot::{
    BreakdownFetchFn, BreakdownSlice, BreakdownUnit, FetchFn as GrokbotFetch, GrokbotEntitlement,
    GrokbotPollerConfig, GrokbotPublisher, GrokbotSnapshot, GrokbotUsageBreakdown,
    GrokbotUsagePoller, GrokbotUsageSource,
};

fn cursor_poller(env: &Env) -> Arc<CursorUsagePoller> {
    let clock = env.dyn_clock();
    let start = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).single();
    let end = Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).single();
    let fetch: CursorFetch = Arc::new(move |_cookie| {
        Box::pin(async move {
            Ok(CursorFetchResult {
                snapshot: CursorSnapshot {
                    fetched_at: t0(),
                    used_percent: 30.0,
                    pools: vec![CursorPoolUsage {
                        kind: PoolKind::Total,
                        used_percent: 30.0,
                        resets_at: end,
                    }],
                    billing_cycle_start: start,
                    billing_cycle_end: end,
                    membership_type: Some("pro".into()),
                    plan_used_usd: Some(6.0),
                    plan_limit_usd: Some(20.0),
                    on_demand_enabled: false,
                    on_demand_used_usd: None,
                    on_demand_limit_usd: None,
                    cost_stats: None,
                    account_email: None,
                    daily_tokens: BTreeMap::new(),
                },
                hourly: CursorDayHourlyUsage::empty(
                    Utc.with_ymd_and_hms(2026, 10, 14, 6, 0, 0)
                        .single()
                        .expect("instant"),
                ),
                estimated_weight_by_day: BTreeMap::new(),
            })
        })
    });
    CursorUsagePoller::new(CursorPollerConfig {
        session: env.auth(ProviderId::Cursor),
        daily: DailyQuotaDeltaStore::new(
            Arc::new(InMemoryStringStore::default()),
            "cursor_daily_usage",
            clock.clone(),
        ),
        clock,
        fetch,
        clear_event_cache: Arc::new(|| {}),
        is_enabled: Arc::new(|| true),
        poll_interval: Arc::new(|_| Some(Duration::from_secs(3600))),
    })
}

/// Cursor is empty until polled, then carries the plan snapshot and bars.
#[tokio::test]
async fn test_cursor_source_goes_from_empty_to_a_charted_snapshot() {
    let e = env();
    e.sign_in(ProviderId::Cursor);
    let p = cursor_poller(&e);
    let source = cursor_source(&p);
    let clock = e.dyn_clock();
    assert_eq!(
        source(&query(clock.as_ref(), 0, Settings::default())),
        SnapshotView::Cursor(None)
    );
    p.refresh_now().await;
    let SnapshotView::Cursor(Some(input)) = source(&query(clock.as_ref(), 0, Settings::default()))
    else {
        panic!("snapshot expected after a poll");
    };
    assert!(input.daily.is_some(), "monthly bars are charted");
}

/// Cursor's hourly source reports its local day, quota and token hours, and
/// nothing before the first poll.
#[tokio::test]
async fn test_cursor_hourly_reports_day_quota_and_tokens() {
    let e = env();
    e.sign_in(ProviderId::Cursor);
    let p = cursor_poller(&e);
    let hourly = cursor_hourly(&p);
    let clock = e.dyn_clock();
    let cal = Calendar::local(clock.as_ref());
    assert!(hourly(&cal).is_none());
    p.refresh_now().await;
    let Some(TodayPart::Cursor(day)) = hourly(&cal) else {
        panic!("cursor part");
    };
    assert_eq!(day.quota_weights.len(), 24);
    assert_eq!(day.hour_tokens.len(), 24);
    assert_eq!(day.day.to_string(), "2026-10-14");
}

fn grokbot_poller(env: &Env, with_breakdown: bool) -> Arc<GrokbotUsagePoller> {
    let clock = env.dyn_clock();
    let store: Arc<dyn TextStore> = Arc::new(InMemoryStringStore::default());
    let fetch: GrokbotFetch = Arc::new(move |_cookie, _email| {
        Box::pin(async move {
            Ok(GrokbotSnapshot {
                fetched_at: t0(),
                used_percent: 20.0,
                period_start: Utc.with_ymd_and_hms(2026, 10, 12, 0, 0, 0).single(),
                resets_at: Utc.with_ymd_and_hms(2026, 10, 19, 0, 0, 0).single(),
                entitlement: GrokbotEntitlement::Cursor,
                account_email: None,
                has_included_allowance: true,
            })
        })
    });
    let breakdown: Option<BreakdownFetchFn> = with_breakdown.then(|| {
        Arc::new(move |_cookie: String| {
            Box::pin(async move {
                Ok(Some(GrokbotUsageBreakdown {
                    unit: BreakdownUnit::Usd,
                    period_start: None,
                    period_end: None,
                    slices: vec![BreakdownSlice {
                        source: GrokbotUsageSource::User,
                        value: 1.5,
                    }],
                }))
            }) as tokenmon_core::BoxFuture<'static, _>
        }) as BreakdownFetchFn
    });
    GrokbotUsagePoller::new(GrokbotPollerConfig {
        session: env.auth(ProviderId::Grokbot),
        publisher: GrokbotPublisher::new(&store, clock.clone()),
        clock,
        fetch,
        fetch_breakdown: breakdown,
        is_enabled: Arc::new(|| true),
        poll_interval: Arc::new(|_| Some(Duration::from_secs(3600))),
    })
}

/// Grokbot is empty until polled, then carries the snapshot and the
/// per-source breakdown.
#[tokio::test]
async fn test_grokbot_source_includes_the_breakdown_after_a_poll() {
    let e = env();
    e.sign_in(ProviderId::Grokbot);
    let p = grokbot_poller(&e, true);
    let source = grokbot_source(&p);
    let clock = e.dyn_clock();
    assert_eq!(
        source(&query(clock.as_ref(), 0, Settings::default())),
        SnapshotView::Grokbot(None)
    );
    p.refresh_now().await;
    let SnapshotView::Grokbot(Some(input)) = source(&query(clock.as_ref(), 0, Settings::default()))
    else {
        panic!("snapshot expected after a poll");
    };
    assert!(input.daily.is_some());
    let items = input.breakdown.expect("breakdown shown");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].amount, 1.5);
}

/// Without a breakdown fetch the Grokbot view simply has none.
#[tokio::test]
async fn test_grokbot_source_has_no_breakdown_when_none_is_fetched() {
    let e = env();
    e.sign_in(ProviderId::Grokbot);
    let p = grokbot_poller(&e, false);
    p.refresh_now().await;
    let clock = e.dyn_clock();
    let SnapshotView::Grokbot(Some(input)) =
        grokbot_source(&p)(&query(clock.as_ref(), 0, Settings::default()))
    else {
        panic!("snapshot expected");
    };
    assert!(input.breakdown.is_none());
}

/// Grokbot's hourly source stamps the publisher's day with 24 weights.
#[tokio::test]
async fn test_grokbot_hourly_reports_day_with_24_weights() {
    let e = env();
    let p = grokbot_poller(&e, false);
    let clock = e.dyn_clock();
    let cal = Calendar::local(clock.as_ref());
    let Some(TodayPart::Grokbot(day)) = grokbot_hourly(&p)(&cal) else {
        panic!("grokbot part");
    };
    assert_eq!(day.weights.len(), 24);
}
