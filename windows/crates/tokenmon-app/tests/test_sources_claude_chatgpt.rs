//! The Claude and ChatGPT typed sources over pollers fed by scripted fetches.

mod common;

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use common::{Env, env, query};
use tokenmon_app::sources::{
    SnapshotView, TodayPart, chatgpt_source, claude_hourly, claude_source,
};
use tokenmon_core::ProviderId;
use tokenmon_core::ProviderUsagePoller;
use tokenmon_core::settings::{PanelSection, Settings};
use tokenmon_core::storage::{DailyQuotaDeltaStore, HourlyDeltaActivityStore, InMemoryStringStore};
use tokenmon_core::usage::Calendar;
use tokenmon_provider_chatgpt::{
    AccessTokenCache, ChatGptPoller, ChatGptPollerConfig, Fetch, FetchFn, parse_usage,
};
use tokenmon_provider_claude::{
    ClaudeCodeUsageScan, ClaudeCodeUsageSummary, ClaudePollerDeps, ClaudeUsagePoller,
    ClaudeUsageResponse, ClaudeUsageWindow, FetchUsageFn, LoadCodeUsageFn, UsageTotals,
};

fn claude_poller(env: &Env, with_code: bool, fail: bool) -> Arc<ClaudeUsagePoller> {
    let clock = env.dyn_clock();
    let store = Arc::new(InMemoryStringStore::default());
    let reset = Utc.with_ymd_and_hms(2026, 10, 19, 6, 0, 0).single();
    let fetch: Arc<FetchUsageFn> = Arc::new(move |_cookie| {
        Box::pin(async move {
            if fail {
                return Err(tokenmon_core::ProviderError::Network(
                    tokenmon_core::ProviderErrorContext::Claude,
                    "down".into(),
                ));
            }
            Ok((
                ClaudeUsageResponse {
                    five_hour: Some(ClaudeUsageWindow {
                        used_percent: 10.0,
                        resets_at: None,
                        scope_name: None,
                    }),
                    seven_day: Some(ClaudeUsageWindow {
                        used_percent: 42.0,
                        resets_at: reset,
                        scope_name: None,
                    }),
                    saved_resets: None,
                },
                Utc.with_ymd_and_hms(2026, 10, 14, 15, 0, 0)
                    .single()
                    .expect("instant"),
            ))
        })
    });
    let load: Arc<LoadCodeUsageFn> = Arc::new(move |_now| {
        Box::pin(async move {
            with_code.then(|| ClaudeCodeUsageScan {
                summary: ClaudeCodeUsageSummary {
                    month: UsageTotals {
                        tokens: 10,
                        cost_usd: 1.0,
                        has_unpriced: false,
                    },
                    all_time: UsageTotals {
                        tokens: 20,
                        cost_usd: 2.0,
                        has_unpriced: false,
                    },
                    month_models: Vec::new(),
                    ..ClaudeCodeUsageSummary::make(
                        &std::collections::BTreeMap::default(),
                        chrono::DateTime::<Utc>::UNIX_EPOCH,
                    )
                },
                local_daily_tokens: std::collections::BTreeMap::default(),
            })
        })
    });
    ClaudeUsagePoller::new(ClaudePollerDeps {
        settings: Arc::clone(&env.settings),
        auth: env.auth(ProviderId::Claude),
        hourly: HourlyDeltaActivityStore::new(store.clone(), "claude_hourly", clock.clone()),
        daily: DailyQuotaDeltaStore::new(store, "claude_daily_usage", clock.clone()),
        clock,
        fetch_usage: fetch,
        load_code_usage: load,
    })
}

/// Before any poll and without local logs the Claude view is empty.
#[tokio::test]
async fn test_claude_source_is_empty_before_the_first_poll() {
    let e = env();
    let poller = claude_poller(&e, false, false);
    let source = claude_source(&poller);
    let clock = e.dyn_clock();
    let view = source(&query(clock.as_ref(), 0, Settings::default()));
    assert_eq!(view, SnapshotView::Claude(None, None));
}

/// A polled Claude snapshot yields the weekly window, the projected reset and
/// the Claude Code section (expanded per the settings).
#[tokio::test]
async fn test_claude_source_builds_snapshot_daily_bars_and_code_section() {
    let e = env();
    e.sign_in(ProviderId::Claude);
    let poller = claude_poller(&e, true, false);
    poller.refresh_now().await;
    let source = claude_source(&poller);
    let clock = e.dyn_clock();
    let SnapshotView::Claude(Some(snap), Some(code)) =
        source(&query(clock.as_ref(), 0, Settings::default()))
    else {
        panic!("expected snapshot and code section");
    };
    assert_eq!(
        snap.seven_day.expect("weekly window").used_percent,
        Some(42.0)
    );
    assert_eq!(
        snap.weekly_resets_at,
        Utc.with_ymd_and_hms(2026, 10, 19, 6, 0, 0).single()
    );
    assert!(snap.daily.is_some(), "weekly bars are charted");
    assert_eq!(code.month.tokens, 10);
    assert!(code.models_expanded, "sections are expanded by default");

    let settings = Settings::default().with_expanded(PanelSection::ClaudeModels, false);
    let SnapshotView::Claude(_, Some(code)) = source(&query(clock.as_ref(), 0, settings)) else {
        panic!("code section expected");
    };
    assert!(!code.models_expanded);
}

/// Local Claude Code usage shows even when the claude.ai fetch fails.
#[tokio::test]
async fn test_claude_source_keeps_code_section_without_a_snapshot() {
    let e = env();
    e.sign_in(ProviderId::Claude);
    let poller = claude_poller(&e, true, true);
    poller.refresh_now().await;
    let source = claude_source(&poller);
    let clock = e.dyn_clock();
    let SnapshotView::Claude(snap, code) = source(&query(clock.as_ref(), 0, Settings::default()))
    else {
        panic!("claude view");
    };
    assert!(snap.is_none());
    assert!(code.is_some());
}

/// Claude's hourly source stamps the poller's local day and 24 weights.
#[tokio::test]
async fn test_claude_hourly_reports_the_local_day_with_24_weights() {
    let e = env();
    let poller = claude_poller(&e, false, false);
    let hourly = claude_hourly(&poller);
    let clock = e.dyn_clock();
    let cal = Calendar::local(clock.as_ref());
    let Some(TodayPart::Claude(day)) = hourly(&cal) else {
        panic!("claude part");
    };
    assert_eq!(day.weights.len(), 24);
    assert_eq!(day.day.to_string(), "2026-10-14");
}

fn chatgpt_poller(env: &Env) -> Arc<ChatGptPoller> {
    let clock = env.dyn_clock();
    let body = r#"{"plan_type":"plus","rate_limit":{"allowed":true,"limit_reached":false,
      "primary_window":{"used_percent":12,"limit_window_seconds":18000,"reset_after_seconds":9000,"reset_at":1787300000},
      "secondary_window":{"used_percent":57.5,"limit_window_seconds":604800,"reset_after_seconds":343143,"reset_at":1792000000}}}"#;
    let fetch: Arc<FetchFn> = Arc::new(move |_cookie, _generation| {
        Box::pin(async move {
            Ok(Fetch {
                response: parse_usage(body.as_bytes()).expect("fixture parses"),
                fetched_at: Utc
                    .with_ymd_and_hms(2026, 10, 14, 15, 0, 0)
                    .single()
                    .expect("instant"),
                set_cookie_headers: Vec::new(),
            })
        })
    });
    ChatGptPoller::new(ChatGptPollerConfig {
        session: env.auth(ProviderId::ChatGpt),
        clock: clock.clone(),
        token_cache: Arc::new(AccessTokenCache::new()),
        fetch,
        is_enabled: Arc::new(|| true),
        poll_interval: Arc::new(|_| Some(Duration::from_secs(3600))),
        daily: DailyQuotaDeltaStore::new(
            Arc::new(InMemoryStringStore::default()),
            "chatgpt_daily_usage",
            clock,
        ),
    })
}

/// ChatGPT: empty before a poll, then plan, windows and daily bars.
#[tokio::test]
async fn test_chatgpt_source_goes_from_empty_to_a_full_snapshot() {
    let e = env();
    e.sign_in(ProviderId::ChatGpt);
    let poller = chatgpt_poller(&e);
    let source = chatgpt_source(&poller);
    let clock = e.dyn_clock();
    assert_eq!(
        source(&query(clock.as_ref(), 0, Settings::default())),
        SnapshotView::ChatGpt(None)
    );
    poller.refresh_now().await;
    let SnapshotView::ChatGpt(Some(snap)) = source(&query(clock.as_ref(), 0, Settings::default()))
    else {
        panic!("snapshot expected after a poll");
    };
    assert_eq!(
        snap.secondary.expect("weekly window").used_percent,
        Some(57.5)
    );
    assert!(snap.allowed);
    assert!(snap.daily.is_some());
}
