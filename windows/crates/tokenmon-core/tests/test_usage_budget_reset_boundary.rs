//! Port of Swift `DailyBudgetResetBoundaryTests` (UTC calendar): weekly bars
//! around a reset instant that an early reset left in place (upstream #55).

#![allow(clippy::unwrap_used, clippy::float_cmp)]

use chrono::{DateTime, TimeZone, Utc};
use tokenmon_core::usage::budget::SpentByDay;
use tokenmon_core::usage::{Calendar, DailyBudget, WeekOffset};

const CAL: Calendar<'static> = Calendar::Fixed(chrono::FixedOffset::east_opt(0).unwrap());

fn date(m: u32, d: u32, h: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, m, d, h, 0, 0).unwrap()
}

fn sod(m: u32, d: u32) -> DateTime<Utc> {
    CAL.start_of_day(date(m, d, 12))
}

/// Grokbot, Nov 2026: the provider began a new period on Sunday Nov 8 but kept
/// the old reset instant (Wednesday Nov 11). The new window's bars stop before
/// the reset day: the original Wed-Tue week with the days before the early
/// reset as prior-window history, not 11 bars running past the reset.
#[test]
fn test_early_reset_with_unchanged_reset_instant_stops_bars_at_reset() {
    let spent: SpentByDay = [(sod(11, 4), 8.0), (sod(11, 7), 53.0), (sod(11, 9), 37.0)]
        .into_iter()
        .collect();
    let days = DailyBudget::build_weekly_window_days(
        100.0,
        7,
        date(11, 11, 12),
        &spent,
        Some(date(11, 8, 23)),
        Some(date(11, 4, 12)),
        WeekOffset::CURRENT,
        date(11, 9, 11),
        &CAL,
    );

    assert_eq!(days.len(), 7);
    assert_eq!(days.first().unwrap().date, sod(11, 4));
    assert_eq!(days.last().unwrap().date, sod(11, 10));
    let prior: Vec<bool> = days.iter().map(|d| d.is_prior_window).collect();
    assert_eq!(prior, [true, true, true, true, false, false, false]);
    let spent: Vec<f64> = days.iter().map(|d| d.spent_usd).collect();
    assert_eq!(spent, [8.0, 0.0, 0.0, 53.0, 0.0, 37.0, 0.0]);
    assert_eq!(
        DailyBudget::weekly_pace_period_start(&days, &CAL),
        Some(sod(11, 8))
    );
}

/// A window start on the reset day itself still gets one bar.
#[test]
fn test_window_start_on_reset_day_keeps_one_bar() {
    let days = DailyBudget::build_weekly_window_days(
        100.0,
        7,
        date(11, 11, 18),
        &SpentByDay::new(),
        Some(date(11, 11, 6)),
        None,
        WeekOffset::CURRENT,
        date(11, 11, 9),
        &CAL,
    );
    let dates: Vec<_> = days.iter().map(|d| d.date).collect();
    assert_eq!(dates, [sod(11, 11)]);
}
