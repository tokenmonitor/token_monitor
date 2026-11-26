//! Overview hour columns, the saved-history store and the midnight rollover.

use chrono::{NaiveDate, TimeZone, Utc};
use tokenmon_app::overview::{OverviewStore, hour_columns, next_midnight};
use tokenmon_core::overview::{OverviewDaySeries, OverviewSeries};
use tokenmon_core::usage::Calendar;
use tokenmon_ui::panels::overview::HourSeries;

fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 7, n).expect("valid date")
}

/// Only series with weight in an hour appear, in stack order, with tokens
/// where the series reports them.
#[test]
fn test_hour_columns_list_active_series_in_stack_order() {
    let mut grok = vec![0.0; 24];
    grok[9] = 2.0;
    let mut cursor = vec![0.0; 24];
    cursor[9] = 1.0;
    let mut tokens = vec![0; 24];
    tokens[9] = 500;
    let series = OverviewDaySeries::default()
        .with_weights(OverviewSeries::Cursor, cursor)
        .with_weights(OverviewSeries::Grok, grok)
        .with_tokens(OverviewSeries::Cursor, tokens);
    let columns = hour_columns(&series);
    assert_eq!(columns.len(), 24);
    assert_eq!(columns[8].len(), 0);
    let nine = &columns[9];
    assert_eq!(nine.len(), 2);
    assert_eq!(nine[0].series, HourSeries::Grok);
    assert_eq!(nine[0].tokens, None);
    assert_eq!(nine[1].series, HourSeries::Cursor);
    assert_eq!(nine[1].tokens, Some(500));
}

/// Recording persists, and a reopened store reads the day back.
#[test]
fn test_store_round_trips_through_the_data_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut w = vec![0.0; 24];
    w[3] = 1.5;
    let series = OverviewDaySeries::default().with_weights(OverviewSeries::Claude, w);
    let store = OverviewStore::open(dir.path());
    assert!(store.record(&series, day(10)));
    assert!(!store.record(&series, day(10)), "no change, no save");
    let reopened = OverviewStore::open(dir.path());
    assert_eq!(reopened.earliest_day(), Some(day(10)));
    let input = reopened.daily_input(0, &OverviewDaySeries::default(), day(10));
    assert_eq!(input.hours[3].len(), 1);
}

/// An earlier offset reads the earlier saved day; a missing day is empty.
#[test]
fn test_daily_input_offset_reads_earlier_days() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut w = vec![0.0; 24];
    w[1] = 1.0;
    let series = OverviewDaySeries::default().with_weights(OverviewSeries::Grok, w);
    let store = OverviewStore::open(dir.path());
    store.record(&series, day(9));
    let yesterday = store.daily_input(-1, &OverviewDaySeries::default(), day(10));
    assert_eq!(yesterday.hours[1].len(), 1);
    let none = store.daily_input(-2, &OverviewDaySeries::default(), day(10));
    assert!(none.hours.iter().all(Vec::is_empty));
    assert_eq!(none.earliest_day, Some(day(9)));
}

/// The next rollover is the following local midnight (DST-safe case lives in
/// tests/dst.rs).
#[test]
fn test_next_midnight_is_the_following_local_midnight() {
    let tz = chrono::FixedOffset::west_opt(7 * 3600).expect("offset");
    let now = Utc
        .with_ymd_and_hms(2026, 7, 10, 3, 0, 0)
        .single()
        .expect("instant");
    // 03:00 UTC is 20:00 the previous local day; local midnight is 07:00 UTC.
    assert_eq!(
        next_midnight(now, &Calendar::fixed(tz)),
        Utc.with_ymd_and_hms(2026, 7, 10, 7, 0, 0)
            .single()
            .expect("instant")
    );
}
