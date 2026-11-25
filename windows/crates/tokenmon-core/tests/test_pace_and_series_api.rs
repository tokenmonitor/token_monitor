//! String forms and small public helpers of the usage pace and overview series
//! modules: allowance periods, pace start dispatch, series names, provider
//! gates and Cursor's partial hourly data.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use tokenmon_core::overview::OverviewSeries;
use tokenmon_core::overview::series::{CursorDay, ProviderGate, TodayInputs, build_today_series};
use tokenmon_core::usage::{AllowancePeriod, Calendar, DailyBudget, DailyBudgetDay};

const CAL: Calendar<'static> = Calendar::Fixed(chrono::FixedOffset::east_opt(0).unwrap());

fn noon(y: i32, m: u32, d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, d, 12, 0, 0)
        .single()
        .expect("valid")
}

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
}

/// Allowance periods roundtrip through their string forms and reject others.
#[test]
fn test_allowance_period_display_and_parse() {
    for p in [AllowancePeriod::Weekly, AllowancePeriod::Monthly] {
        assert_eq!(
            p.to_string().parse::<AllowancePeriod>().expect("roundtrip"),
            p
        );
    }
    assert_eq!(AllowancePeriod::Weekly.to_string(), "weekly");
    assert_eq!(AllowancePeriod::Monthly.to_string(), "monthly");
    let e = "yearly".parse::<AllowancePeriod>().expect_err("unknown");
    assert!(e.to_string().contains("yearly"));
}

/// The dispatcher sends weekly to the first non-prior bar and monthly to the
/// subscription month implied by `resets_at`, else none.
#[test]
fn test_pace_period_start_dispatches_by_period() {
    let bars: Vec<_> = (0..7)
        .map(|o| DailyBudgetDay::new(CAL.start_of_day(noon(2026, 8, 20 + o)), 0.0, 100.0 / 31.0))
        .collect();
    let now = noon(2026, 8, 25);
    let weekly = DailyBudget::pace_period_start(
        AllowancePeriod::Weekly,
        &bars,
        None,
        Some(noon(2026, 9, 16)),
        now,
        &CAL,
    )
    .expect("weekly start");
    assert_eq!(CAL.date_of(weekly), day(2026, 8, 20));
    let monthly = DailyBudget::pace_period_start(
        AllowancePeriod::Monthly,
        &bars,
        None,
        Some(noon(2026, 9, 16)),
        now,
        &CAL,
    )
    .expect("monthly start");
    assert_eq!(CAL.date_of(monthly), day(2026, 8, 16));
    // Without a reset there is nothing to anchor the month to.
    assert_eq!(
        DailyBudget::pace_period_start(AllowancePeriod::Monthly, &bars, None, None, now, &CAL),
        None
    );
}

/// A negative fallback never yields a negative period length.
#[test]
fn test_days_in_period_fallback_is_never_negative() {
    assert_eq!(DailyBudget::days_in_period_from_daily_budget(0.0, -5), 0);
    assert_eq!(DailyBudget::days_in_period_from_daily_budget(0.0, 9), 9);
    assert_eq!(DailyBudget::days_in_period_from_daily_budget(100.0, 9), 1);
}

/// Series names roundtrip in stack order and unknown names are rejected.
#[test]
fn test_overview_series_names_roundtrip() {
    for s in OverviewSeries::STACK_ORDER {
        assert_eq!(s.to_string(), s.as_str());
        assert_eq!(
            s.to_string().parse::<OverviewSeries>().expect("roundtrip"),
            s
        );
        assert_eq!(OverviewSeries::STACK_ORDER[s.index()], s);
    }
    let e = "nope".parse::<OverviewSeries>().expect_err("unknown");
    assert!(e.to_string().contains("nope"));
}

/// The gate requires all three conditions, except OpenCode which only needs
/// to be enabled.
#[test]
fn test_provider_gate_truth_table() {
    for enabled in [false, true] {
        for signed_in in [false, true] {
            for needs_sign_in in [false, true] {
                let g = ProviderGate {
                    enabled,
                    signed_in,
                    needs_sign_in,
                };
                assert_eq!(g.contributes(), enabled && signed_in && !needs_sign_in);
                assert_eq!(g.contributes_without_sign_in(), enabled);
            }
        }
    }
}

/// Cursor contributes weights and tokens independently: a day with only
/// weights, only tokens, both, or the wrong day each land where expected.
#[test]
fn test_cursor_weights_and_tokens_are_independent() {
    let date = day(2026, 8, 25);
    let build = |c: CursorDay| {
        build_today_series(
            &TodayInputs {
                cursor: Some(c),
                ..TodayInputs::default()
            },
            date,
        )
    };
    let weights_only = build(CursorDay {
        day: date,
        quota_weights: vec![1.0; 24],
        hour_tokens: vec![1; 5],
    });
    assert!(weights_only.weights.contains_key(&OverviewSeries::Cursor));
    assert!(!weights_only.tokens.contains_key(&OverviewSeries::Cursor));
    let tokens_only = build(CursorDay {
        day: date,
        quota_weights: vec![1.0; 3],
        hour_tokens: vec![2; 24],
    });
    assert!(!tokens_only.weights.contains_key(&OverviewSeries::Cursor));
    assert_eq!(tokens_only.tokens[&OverviewSeries::Cursor], vec![2; 24]);
    let wrong_day = build(CursorDay {
        day: day(2026, 8, 24),
        quota_weights: vec![1.0; 24],
        hour_tokens: vec![2; 24],
    });
    assert!(wrong_day.weights.is_empty() && wrong_day.tokens.is_empty());
    let none = build_today_series(&TodayInputs::default(), date);
    assert!(none.weights.is_empty());
}
