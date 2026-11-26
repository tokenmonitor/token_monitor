//! Daily-chart inputs: navigation state, over-budget flags, pace resolution
//! and Grok's per-product week.

use chrono::{DateTime, TimeZone, Utc};
use tokenmon_app::daily::{ChartContext, daily_input, grok_week_input, noun_for};
use tokenmon_core::ProviderId;
use tokenmon_core::usage::{Calendar, DailyBudgetDay, DailyTokenUsage, UsageSource};
use tokenmon_provider_grok::models::{DailyUsageDay, DailyUsageWeek};
use tokenmon_ui::panels::common::{AllowanceNoun, FooterKind};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 8, 12, 0, 0)
        .single()
        .expect("instant")
}

fn ctx<'a>(cal: &'a Calendar<'a>, offset: i32) -> ChartContext<'a> {
    ChartContext {
        provider: ProviderId::Claude,
        noun: AllowanceNoun::Weekly,
        period_used_percent: Some(30.0),
        period_start: None,
        resets_at: None,
        week_offset: offset,
        now: now(),
        calendar: cal,
    }
}

fn bars() -> Vec<DailyBudgetDay> {
    (0..7)
        .map(|i| {
            DailyBudgetDay::new(
                Utc.with_ymd_and_hms(2026, 7, 6 + i, 0, 0, 0)
                    .single()
                    .expect("instant"),
                if i == 0 { 40.0 } else { 1.0 },
                100.0 / 7.0,
            )
        })
        .collect()
}

fn week(days: usize, estimated: bool, has_data: bool) -> DailyUsageWeek {
    DailyUsageWeek {
        week_start: now(),
        week_end: now(),
        days: (0..days)
            .map(|i| DailyUsageDay {
                day_start: now() + chrono::Duration::days(i64::try_from(i).expect("small")),
                weekday_symbol: "Wed".into(),
                day_of_month: "8".into(),
                segments: Vec::new(),
                is_today: i == 0,
                is_after_reset: false,
            })
            .collect(),
        legend_products: Vec::new(),
        has_daily_data: has_data,
        is_estimated: estimated,
    }
}

/// No bars means no chart card.
#[test]
fn test_empty_bars_give_no_chart() {
    let cal = Calendar::utc();
    assert!(daily_input(&[], &ctx(&cal, 0)).is_none());
}

/// The current period shows the live footer and cannot go forward; the
/// over-budget flag follows spent versus allowance.
#[test]
fn test_current_period_navigation_and_over_budget() {
    let cal = Calendar::utc();
    let input = daily_input(&bars(), &ctx(&cal, 0)).expect("chart");
    assert!(input.show_live_footer);
    assert!(!input.can_go_next);
    assert!(input.has_navigation);
    assert!(input.days[0].is_over_budget);
    assert!(!input.days[1].is_over_budget);
    assert!(input.pace.is_some());
}

/// An earlier period drops pace and the live footer and allows forward.
#[test]
fn test_earlier_period_hides_live_footer() {
    let cal = Calendar::utc();
    let input = daily_input(&bars(), &ctx(&cal, -1)).expect("chart");
    assert!(!input.show_live_footer);
    assert!(input.can_go_next);
    assert!(input.pace.is_none());
}

/// Cursor and OpenCode say monthly; the rest weekly.
#[test]
fn test_noun_per_provider() {
    assert_eq!(noun_for(ProviderId::Cursor), AllowanceNoun::Monthly);
    assert_eq!(noun_for(ProviderId::OpenCode), AllowanceNoun::Monthly);
    assert_eq!(noun_for(ProviderId::Grok), AllowanceNoun::Weekly);
    assert_eq!(noun_for(ProviderId::Claude), AllowanceNoun::Weekly);
}

/// Token counts carry over with negatives floored at zero.
#[test]
fn test_tokens_carry_over() {
    let cal = Calendar::utc();
    let mut days = bars();
    days[0].tokens = Some(DailyTokenUsage {
        input_tokens: 10,
        output_tokens: -5,
        ..DailyTokenUsage::new(UsageSource::Account)
    });
    let input = daily_input(&days, &ctx(&cal, 0)).expect("chart");
    let t = input.days[0].tokens.as_ref().expect("tokens");
    assert_eq!((t.input, t.output), (10, 0));
}

/// Without a used percent there is nothing to pace.
#[test]
fn test_no_used_percent_means_no_pace() {
    let cal = Calendar::utc();
    let context = ChartContext {
        period_used_percent: None,
        ..ctx(&cal, 0)
    };
    assert!(
        daily_input(&bars(), &context)
            .expect("chart")
            .pace
            .is_none()
    );
}

/// A weekly pool whose bars are all prior-window has no pace start, hence no
/// pace.
#[test]
fn test_weekly_pool_with_only_prior_window_bars_has_no_pace() {
    let cal = Calendar::utc();
    let mut days = bars();
    for d in &mut days {
        d.is_prior_window = true;
    }
    assert!(
        daily_input(&days, &ctx(&cal, 0))
            .expect("chart")
            .pace
            .is_none()
    );
}

/// A monthly pool paces from its known cycle start.
#[test]
fn test_monthly_pool_paces_from_its_known_start() {
    let cal = Calendar::utc();
    let context = ChartContext {
        noun: AllowanceNoun::Monthly,
        period_start: Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).single(),
        resets_at: Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).single(),
        ..ctx(&cal, 0)
    };
    let input = daily_input(&bars(), &context).expect("chart");
    assert_eq!(input.noun, AllowanceNoun::Monthly);
    assert!(input.pace.is_some());
}

/// A monthly pool that cannot resolve its start shows no pace.
#[test]
fn test_monthly_pool_without_a_start_has_no_pace() {
    let cal = Calendar::utc();
    let context = ChartContext {
        noun: AllowanceNoun::Monthly,
        ..ctx(&cal, 0)
    };
    let mut days = bars();
    for d in &mut days {
        d.is_prior_window = true;
    }
    assert!(daily_input(&days, &context).expect("chart").pace.is_none());
}

/// Grok's week has no chart without days.
#[test]
fn test_grok_week_without_days_has_no_chart() {
    let cal = Calendar::utc();
    assert!(grok_week_input(&week(0, false, true), &ctx(&cal, 0)).is_none());
}

/// Grok's footer notes an estimate when the week is estimated or has no real
/// daily deltas, and not otherwise.
#[test]
fn test_grok_week_estimate_note_follows_the_week_flags() {
    let cal = Calendar::utc();
    let note = |estimated, has_data| {
        let input = grok_week_input(&week(7, estimated, has_data), &ctx(&cal, 0)).expect("chart");
        match input.footer_kind {
            FooterKind::GrokWeek { show_estimate_note } => show_estimate_note,
            other @ FooterKind::Generic => panic!("unexpected footer {other:?}"),
        }
    };
    assert!(note(true, true));
    assert!(note(false, false));
    assert!(!note(false, true));
}

/// Browsing an earlier Grok week hides pace and the live footer.
#[test]
fn test_grok_earlier_week_hides_pace() {
    let cal = Calendar::utc();
    let input = grok_week_input(&week(7, false, true), &ctx(&cal, -1)).expect("chart");
    assert!(input.pace.is_none());
    assert!(!input.show_live_footer);
    assert!(input.can_go_next);
}
