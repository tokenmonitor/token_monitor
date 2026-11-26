//! Edge cases of the daily-usage builder: how the live `current` sample is
//! merged with history, empty breakdowns, and windows with later samples.
#![allow(clippy::unwrap_used, clippy::assert_is_empty)]

mod common;

use chrono::{DateTime, Datelike, TimeDelta, Utc};
use common::{clock_at, utc};
use tokenmon_provider_grok::calendar::{DayCalendar, LocalCalendar};
use tokenmon_provider_grok::daily::week;
use tokenmon_provider_grok::models::{
    DailyUsageDay, DailyUsageWeek, ProductUsage, WeeklyUsageSnapshot,
};

fn snap(
    at: DateTime<Utc>,
    used: f64,
    resets: Option<DateTime<Utc>>,
    products: &[(&str, f64)],
) -> WeeklyUsageSnapshot {
    WeeklyUsageSnapshot::new(at, used)
        .with_resets_at(resets)
        .with_products(
            products
                .iter()
                .map(|(id, pct)| ProductUsage::new(id, *id, *pct))
                .collect(),
        )
}

fn build(
    history: &[WeeklyUsageSnapshot],
    current: Option<&WeeklyUsageSnapshot>,
    offset: i64,
    resets: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<DailyUsageWeek> {
    let clock = clock_at(now);
    let cal = LocalCalendar::new(clock.as_ref());
    week(history, current, offset, resets, &cal, now)
}

fn day_on(w: &DailyUsageWeek, at: DateTime<Utc>) -> &DailyUsageDay {
    let clock = clock_at(at);
    let cal = LocalCalendar::new(clock.as_ref());
    w.days
        .iter()
        .find(|d| cal.same_day(d.day_start, at))
        .expect("day is inside the window")
}

fn near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.5,
        "{actual} not near {expected}"
    );
}

/// Without a live sample the chart is built from history alone.
#[test]
fn test_week_without_current_uses_history_only() {
    let now = utc(2026, 7, 15, 12, 0);
    let reset = utc(2026, 7, 16, 18, 57);
    let history = [
        snap(
            utc(2026, 7, 13, 12, 0),
            10.0,
            Some(reset),
            &[("chat", 10.0)],
        ),
        snap(
            utc(2026, 7, 14, 12, 0),
            25.0,
            Some(reset),
            &[("chat", 25.0)],
        ),
    ];
    let w = build(&history, None, 0, Some(reset), now).unwrap();
    near(day_on(&w, utc(2026, 7, 14, 0, 0)).total_percent(), 15.0);
}

/// A live sample hours newer than history, with different usage, is a real
/// new sample and extends the chart (it is not a near-duplicate).
#[test]
fn test_distinct_newer_current_sample_is_appended() {
    let now = utc(2026, 7, 15, 12, 0);
    let reset = utc(2026, 7, 16, 18, 57);
    let history = [
        snap(
            utc(2026, 7, 13, 12, 0),
            10.0,
            Some(reset),
            &[("chat", 10.0)],
        ),
        snap(
            utc(2026, 7, 14, 12, 0),
            20.0,
            Some(reset),
            &[("chat", 20.0)],
        ),
    ];
    let current = snap(now, 35.0, Some(reset), &[("chat", 35.0)]);
    let w = build(&history, Some(&current), 0, Some(reset), now).unwrap();
    near(day_on(&w, now).total_percent(), 15.0);
}

/// Same used% but more than a minute later is still a new sample.
#[test]
fn test_same_percent_after_a_minute_is_not_a_duplicate() {
    let now = utc(2026, 7, 15, 12, 5);
    let reset = utc(2026, 7, 16, 18, 57);
    let history = [
        snap(
            utc(2026, 7, 14, 12, 0),
            20.0,
            Some(reset),
            &[("chat", 20.0)],
        ),
        snap(
            utc(2026, 7, 15, 12, 0),
            30.0,
            Some(reset),
            &[("chat", 30.0)],
        ),
    ];
    let current = snap(now, 30.0, Some(reset), &[("chat", 30.0)]);
    let w = build(&history, Some(&current), 0, Some(reset), now).unwrap();
    near(day_on(&w, now).total_percent(), 10.0);
}

/// Within a minute and with identical usage but across local midnight, the
/// current sample opens a new day rather than being dropped.
#[test]
fn test_near_duplicate_across_midnight_is_kept() {
    let reset = utc(2026, 7, 20, 18, 57);
    let last_at = utc(2026, 7, 14, 23, 59) + TimeDelta::seconds(40);
    let now = last_at + TimeDelta::seconds(30);
    let history = [
        snap(
            utc(2026, 7, 13, 12, 0),
            10.0,
            Some(reset),
            &[("chat", 10.0)],
        ),
        snap(last_at, 30.0, Some(reset), &[("chat", 30.0)]),
    ];
    let current = snap(now, 30.0, Some(reset), &[("chat", 30.0)]);
    let w = build(&history, Some(&current), 0, Some(reset), now).unwrap();
    assert_eq!(now.day(), 15);
    let day = day_on(&w, now);
    assert!(day.is_today, "the post-midnight day is today");
}

/// A live sample older than the newest history entry is ignored.
#[test]
fn test_older_current_sample_is_ignored() {
    let now = utc(2026, 7, 15, 12, 0);
    let reset = utc(2026, 7, 16, 18, 57);
    let history = [
        snap(
            utc(2026, 7, 13, 12, 0),
            10.0,
            Some(reset),
            &[("chat", 10.0)],
        ),
        snap(
            utc(2026, 7, 14, 12, 0),
            25.0,
            Some(reset),
            &[("chat", 25.0)],
        ),
    ];
    let stale = snap(utc(2026, 7, 13, 6, 0), 99.0, Some(reset), &[("chat", 99.0)]);
    let w = build(&history, Some(&stale), 0, Some(reset), now).unwrap();
    near(day_on(&w, utc(2026, 7, 14, 0, 0)).total_percent(), 15.0);
    assert!(w.days.iter().all(|d| d.total_percent() < 50.0));
}

/// The very first day of a period with usage but no per-product breakdown is
/// shown as a single `other` segment carrying the whole headline.
#[test]
fn test_period_start_without_breakdown_shows_other_segment() {
    let now = utc(2026, 7, 16, 20, 0);
    let reset = utc(2026, 7, 23, 18, 57);
    let history = [snap(now, 12.0, Some(reset), &[])];
    let w = build(&history, None, 0, Some(reset), now).unwrap();
    let day = day_on(&w, now);
    assert_eq!(day.segments.len(), 1);
    assert_eq!(day.segments[0].product_id, "other");
    near(day.total_percent(), 12.0);
}

/// A breakdown whose products are all at zero is treated as having no usable
/// weights, so the day falls back to one `other` segment.
#[test]
fn test_period_start_with_zero_weight_products_shows_other_segment() {
    let now = utc(2026, 7, 16, 20, 0);
    let reset = utc(2026, 7, 23, 18, 57);
    let history = [snap(
        now,
        8.0,
        Some(reset),
        &[("chat", 0.0), ("build", 0.0)],
    )];
    let w = build(&history, None, 0, Some(reset), now).unwrap();
    let day = day_on(&w, now);
    assert_eq!(day.segments.len(), 1);
    assert_eq!(day.segments[0].product_id, "other");
}

/// A first sample at 0% on the period start paints nothing.
#[test]
fn test_period_start_with_zero_usage_paints_no_bar() {
    let now = utc(2026, 7, 16, 20, 0);
    let reset = utc(2026, 7, 23, 18, 57);
    let history = [snap(now, 0.0, Some(reset), &[])];
    let w = build(&history, None, 0, Some(reset), now).unwrap();
    assert!(day_on(&w, now).segments.is_empty());
    assert!(!w.has_daily_data);
}

/// A mid-window day at 0% right after a period start paints nothing.
#[test]
fn test_zero_usage_mid_window_day_after_rollover_paints_no_bar() {
    let now = utc(2026, 7, 18, 12, 0);
    let old = Some(utc(2026, 7, 16, 18, 57));
    let new = utc(2026, 7, 23, 18, 57);
    let history = [
        snap(utc(2026, 7, 15, 12, 0), 70.0, old, &[("build", 70.0)]),
        snap(utc(2026, 7, 18, 12, 0), 0.0, Some(new), &[]),
    ];
    let w = build(&history, None, 0, Some(new), now).unwrap();
    assert!(day_on(&w, now).segments.is_empty());
}

/// For a past page, a live `resets_at` landing on the day after the window is
/// accepted as the period anchor when no sample is inside the window.
#[test]
fn test_past_week_without_samples_anchors_to_live_reset() {
    let now = utc(2026, 7, 20, 12, 0);
    let live = utc(2026, 7, 23, 18, 57);
    let history = [snap(
        utc(2026, 7, 20, 11, 0),
        10.0,
        Some(live),
        &[("chat", 10.0)],
    )];
    let w = build(&history, None, -1, Some(live), now).unwrap();
    assert!(!w.has_daily_data);
    assert_eq!(w.days.len(), 7);
}

/// Samples after the displayed window (here with no `resets_at`) do not leak
/// into a past page.
#[test]
fn test_samples_after_window_do_not_appear_on_past_page() {
    let now = utc(2026, 7, 20, 12, 0);
    let live = utc(2026, 7, 23, 18, 57);
    let history = [
        snap(utc(2026, 7, 10, 12, 0), 10.0, None, &[("chat", 10.0)]),
        snap(utc(2026, 7, 11, 12, 0), 30.0, None, &[("chat", 30.0)]),
        snap(utc(2026, 7, 19, 12, 0), 5.0, None, &[("chat", 5.0)]),
    ];
    let w = build(&history, None, -1, Some(live), now).unwrap();
    near(day_on(&w, utc(2026, 7, 11, 0, 0)).total_percent(), 20.0);
    assert!(w.days.iter().all(|d| d.total_percent() < 25.0));
}
