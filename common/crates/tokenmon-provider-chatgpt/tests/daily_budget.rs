//! Ports of the weekly-window and daily-bar cases of
//! `ChatGPTUsagePollerTests.swift`.
#![allow(clippy::unwrap_used, clippy::unused_async)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use common::FakeSession;
use tokenmon_core::polling::PollOutcome;
use tokenmon_core::storage::{DailyQuotaDeltaStore, InMemoryStringStore};
use tokenmon_core::usage::WeekOffset;
use tokenmon_core::{AuthEvent, Clock, ManualClock};
use tokenmon_provider_chatgpt::{
    AccessTokenCache, ChatGptPoller, ChatGptPollerConfig, ChatGptSnapshot, Fetch, FetchFn,
    UsageResponse, UsageWindow,
};

const COOKIE: &str = "__Secure-next-auth.session-token=live";
const NOW: i64 = 1_790_000_000;

fn now() -> DateTime<Utc> {
    Utc.timestamp_opt(NOW, 0).unwrap()
}

fn window(used: f64, resets_in: Option<i64>, seconds: Option<i64>) -> UsageWindow {
    UsageWindow {
        used_percent: used,
        resets_at: resets_in.map(|s| now() + chrono::Duration::seconds(s)),
        window_seconds: seconds,
    }
}

fn response(primary: Option<UsageWindow>, secondary: Option<UsageWindow>) -> UsageResponse {
    UsageResponse {
        plan_name: Some("plus".into()),
        allowed: true,
        limit_reached: false,
        primary,
        secondary,
        saved_resets: None,
    }
}

fn snapshot(primary: Option<UsageWindow>, secondary: Option<UsageWindow>) -> ChatGptSnapshot {
    ChatGptSnapshot::new(response(primary, secondary), now())
}

fn weekly_response(percent: f64, resets_in: i64) -> UsageResponse {
    response(
        Some(window(5.0, Some(3_600), Some(18_000))),
        Some(window(
            percent,
            Some(resets_in),
            Some(ChatGptSnapshot::WEEK_SECONDS),
        )),
    )
}

struct Rig {
    poller: Arc<ChatGptPoller>,
    session: Arc<FakeSession>,
    text: Arc<InMemoryStringStore>,
    clock: Arc<dyn Clock>,
}

impl Rig {
    /// A second view of the persisted daily store (what the poller wrote).
    fn store(&self) -> DailyQuotaDeltaStore {
        DailyQuotaDeltaStore::new(
            self.text.clone(),
            "chatgpt_daily_usage",
            Arc::clone(&self.clock),
        )
    }
}

fn rig(body: UsageResponse) -> Rig {
    let session = FakeSession::signed_in(COOKIE);
    let clock: Arc<dyn Clock> = Arc::new(ManualClock::from_unix(NOW));
    let text = Arc::new(InMemoryStringStore::default());
    let fetch: Arc<FetchFn> = Arc::new(move |_cookie, _generation| {
        let body = body.clone();
        Box::pin(async move {
            Ok(Fetch {
                response: body,
                fetched_at: now(),
                set_cookie_headers: vec![],
            })
        })
    });
    let poller = ChatGptPoller::new(ChatGptPollerConfig {
        session: session.clone(),
        clock: Arc::clone(&clock),
        token_cache: Arc::new(AccessTokenCache::new()),
        fetch,
        is_enabled: Arc::new(|| true),
        poll_interval: Arc::new(|_| Some(Duration::from_secs(3_600))),
        daily: DailyQuotaDeltaStore::new(text.clone(), "chatgpt_daily_usage", Arc::clone(&clock)),
    });
    Rig {
        poller,
        session,
        text,
        clock,
    }
}

/// The seven-day window is preferred, including as a free plan's primary.
#[test]
fn test_weekly_window_prefers_the_seven_day_window() {
    let week = window(20.0, None, Some(ChatGptSnapshot::WEEK_SECONDS));
    let five_hour = window(5.0, None, Some(18_000));
    assert_eq!(
        snapshot(Some(five_hour), Some(week.clone())).weekly_window(),
        Some(&week)
    );
    // A free plan reports the weekly window as the primary one.
    assert_eq!(
        snapshot(Some(week.clone()), None).weekly_window(),
        Some(&week)
    );
}

/// The secondary window stands in only when it reports no length.
#[test]
fn test_weekly_window_falls_back_to_secondary_only_without_length() {
    let unlabeled = window(30.0, None, None);
    assert_eq!(
        snapshot(None, Some(unlabeled.clone())).weekly_window(),
        Some(&unlabeled)
    );
    let daily = window(30.0, None, Some(86_400));
    assert_eq!(snapshot(None, Some(daily)).weekly_window(), None);
    assert_eq!(
        snapshot(Some(window(1.0, None, None)), None).weekly_window(),
        None
    );
}

/// A refresh records the weekly used % and publishes seven bars.
#[tokio::test]
async fn test_refresh_records_weekly_usage_and_publishes_seven_bars() {
    let resets_at = now() + chrono::Duration::days(3);
    let rig = rig(weekly_response(42.0, 3 * 86_400));
    assert_eq!(rig.poller.perform_refresh().await, PollOutcome::Success);

    assert_eq!(rig.poller.daily_budget_days().unwrap().len(), 7);
    let store = rig.store();
    assert!((store.last_used_percent().unwrap() - 42.0).abs() < 1e-3);
    assert_eq!(store.window_resets_at(), Some(resets_at));
    assert_eq!(rig.poller.weekly_resets_at(now()), Some(resets_at));
    assert_eq!(
        rig.poller.daily_budget_days_at(WeekOffset::CURRENT).len(),
        7
    );
}

/// Without a weekly window nothing is recorded and there are no bars.
#[tokio::test]
async fn test_refresh_without_weekly_window_shows_no_bars() {
    let rig = rig(response(
        Some(window(10.0, Some(3_600), Some(18_000))),
        None,
    ));
    assert_eq!(rig.poller.perform_refresh().await, PollOutcome::Success);
    assert_eq!(rig.poller.daily_budget_days(), Some(Vec::new()));
    assert_eq!(rig.store().last_used_percent(), None);
}

/// Signing out keeps the daily history for the same account; a different
/// account signing in clears it.
#[tokio::test]
async fn test_account_switch_clears_daily_history_but_sign_out_keeps_it() {
    let rig = rig(weekly_response(12.0, 2 * 86_400));
    assert_eq!(rig.poller.perform_refresh().await, PollOutcome::Success);
    assert!(rig.store().last_used_percent().is_some());

    rig.session.fire(AuthEvent::SignedOut);
    assert_eq!(rig.poller.daily_budget_days(), None);
    assert!(rig.store().last_used_percent().is_some());

    rig.session.fire(AuthEvent::AccountReset);
    assert_eq!(rig.store().last_used_percent(), None);
    assert!(rig.store().spent_by_day().is_empty());
}
