//! Port of `OpenRouterDailyBudgetTests.swift`.
#![allow(clippy::unwrap_used)]

use chrono::{DateTime, Datelike, TimeZone, Utc};
use tokenmon_core::usage::{Calendar, DailyBudgetDay, WeekOffset};
use tokenmon_provider_openrouter::{
    ActivityRow, ChartKind, CreditsData, KeyData, LimitReset, NoCostEstimator,
    OpenRouterDailyChart, OpenRouterSnapshot,
};

/// Saturday 31 Jan 2026, 23:30 UTC, inside the Monday-Sunday week that ends 2 Feb.
fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 31, 23, 30, 0).unwrap()
}

fn utc_midnight(y: i32, m: u32, d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()
}

fn key(limit: Option<f64>, reset: Option<&str>) -> KeyData {
    KeyData {
        label: Some("Test Key".into()),
        usage: 20.0,
        usage_daily: None,
        usage_weekly: None,
        usage_monthly: None,
        limit,
        limit_remaining: None,
        limit_reset: reset.map(str::to_owned),
        is_free_tier: Some(false),
        is_management_key: Some(true),
    }
}

fn row(date: &str, usage: f64) -> ActivityRow {
    ActivityRow {
        date: date.to_owned(),
        model: "test".to_owned(),
        provider_name: None,
        usage,
        requests: 1,
        prompt_tokens: 1,
        completion_tokens: 1,
        reasoning_tokens: 0,
    }
}

fn build(
    key: &KeyData,
    credits: Option<CreditsData>,
    activity: Option<&[ActivityRow]>,
) -> OpenRouterSnapshot {
    OpenRouterSnapshot::build(key, credits, activity, now(), &NoCostEstimator)
}

fn day_of(days: &[DailyBudgetDay], day: u32) -> &DailyBudgetDay {
    days.iter().find(|d| d.date.day() == day).unwrap()
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-3, "{a} != {b}");
}

/// A weekly key limit keeps each UTC day's spend as a percent of that limit.
#[test]
fn test_weekly_key_limit_uses_that_weeks_spend() {
    let activity = [row("2026-01-31", 10.0), row("2026-01-31T00:00:00Z", 4.0)];
    let snap = build(&key(Some(70.0), Some("weekly")), None, Some(&activity));
    assert_eq!(snap.limit_reset, Some(LimitReset::Weekly));
    close(snap.spend_by_utc_day[&now().date_naive()], 14.0);

    let chart = OpenRouterDailyChart::make(&snap, now()).unwrap();
    assert_eq!(chart.kind, ChartKind::Weekly);
    close(
        chart.period_used_percent.unwrap(),
        snap.used_percent().unwrap(),
    );
    let days = chart.days(WeekOffset::CURRENT, &Calendar::utc());
    assert_eq!(days.len(), 7);
    close(day_of(&days, 31).spent_usd, 14.0 / 70.0 * 100.0);
    close(day_of(&days, 31).budget_usd, 100.0 / 7.0);
    assert_eq!(days[0].date.day(), 26);
    assert_eq!(chart.resets_at, Some(utc_midnight(2026, 2, 2)));
}

/// A daily cap compares each UTC day with that day's own limit, not a weekly share.
#[test]
fn test_daily_cap_is_each_days_own_limit() {
    let mut k = key(Some(10.0), Some("daily"));
    k.usage_daily = Some(5.0);
    let snap = build(&k, None, None);
    let chart = OpenRouterDailyChart::make(&snap, now()).unwrap();
    assert_eq!(chart.kind, ChartKind::DailyCap);
    assert_eq!(chart.period_used_percent, None);
    let days = chart.days(WeekOffset::CURRENT, &Calendar::utc());
    assert_eq!(days.len(), 7);
    let today = days.last().unwrap();
    assert_eq!(today.date.day(), 31);
    close(today.spent_usd, 50.0);
    close(today.budget_usd, 100.0);
    assert!(days[..6].iter().all(|d| d.spent_usd == 0.0));
}

/// Account credits keep the credit pool. The key's own reset is not the chart's
/// window, and today's `/key` spend fills a day activity did not report.
#[test]
fn test_account_credits_ignore_the_key_reset() {
    let mut k = key(Some(10.0), Some("Weekly"));
    k.usage_daily = Some(3.0);
    let credits = CreditsData {
        total_credits: 200.0,
        total_usage: 40.0,
    };
    let snap = build(&k, Some(credits), Some(&[row("2026-01-30", 8.0)]));
    let by_day = &snap.spend_by_utc_day;
    close(
        by_day[&Utc
            .with_ymd_and_hms(2026, 1, 30, 0, 0, 0)
            .unwrap()
            .date_naive()],
        8.0,
    );
    close(by_day[&now().date_naive()], 3.0);
    let chart = OpenRouterDailyChart::make(&snap, now()).unwrap();
    assert_eq!(chart.kind, ChartKind::Weekly);
    assert_eq!(chart.period_used_percent, None);
    assert_eq!(chart.resets_at, None);
    let days = chart.days(WeekOffset::CURRENT, &Calendar::utc());
    close(day_of(&days, 30).spent_usd, 8.0 / 200.0 * 100.0);
    close(day_of(&days, 31).spent_usd, 3.0 / 200.0 * 100.0);
}

/// A monthly key limit paces the subscription month and keeps that month's
/// daily percents.
#[test]
fn test_monthly_key_limit_uses_the_subscription_month() {
    let mut k = key(Some(50.0), Some("monthly"));
    k.usage_monthly = Some(20.0);
    let snap = build(&k, None, Some(&[row("2026-01-31", 10.0)]));
    let chart = OpenRouterDailyChart::make(&snap, now()).unwrap();
    assert_eq!(chart.kind, ChartKind::Monthly);
    close(
        chart.period_used_percent.unwrap(),
        snap.used_percent().unwrap(),
    );
    assert_eq!(chart.resets_at, Some(utc_midnight(2026, 2, 1)));
    assert_eq!(chart.period_start, Some(utc_midnight(2026, 1, 1)));
    let days = chart.days(WeekOffset::CURRENT, &Calendar::utc());
    assert_eq!(days.len(), 7);
    assert_eq!(days[0].date.day(), 26);
    close(day_of(&days, 31).spent_usd, 10.0 / 50.0 * 100.0);
}

/// An unrecognized reset (hourly) is a cap with no week to pace.
#[test]
fn test_hourly_key_cap_is_not_paced_as_a_week() {
    let snap = build(&key(Some(80.0), Some("hourly")), None, None);
    let chart = OpenRouterDailyChart::make(&snap, now()).unwrap();
    assert_eq!(snap.limit_reset, Some(LimitReset::Other));
    assert_eq!(chart.kind, ChartKind::Weekly);
    assert_eq!(chart.period_used_percent, None);
    assert_eq!(chart.resets_at, None);
}

/// Without a budget there is no chart.
#[test]
fn test_no_budget_has_no_chart() {
    let snap = build(&key(None, None), None, None);
    assert!(OpenRouterDailyChart::make(&snap, now()).is_none());
}

/// Malformed activity dates are skipped and negative spend clamps to zero.
#[test]
fn test_activity_dates_must_be_utc_days() {
    let activity = [
        row("nonsense", 5.0),
        row("2026-1-31", 5.0),
        row("2026-01-30", -3.0),
    ];
    let snap = build(&key(Some(70.0), Some("weekly")), None, Some(&activity));
    assert_eq!(snap.spend_by_utc_day.len(), 1);
    assert!(snap.spend_by_utc_day.values().all(|v| *v == 0.0));
}
