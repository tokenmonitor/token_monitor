//! The Grokbot panel's Usage Breakdown card: its items from the provider's
//! breakdown, and its place above the daily budget with no funding caption.
#![allow(missing_docs, clippy::unwrap_used, clippy::assert_is_empty)]

mod panels_common;

use panels_common::*;
use tokenmon_provider_grokbot::{
    BreakdownSlice, BreakdownUnit, GrokbotUsageBreakdown, GrokbotUsageSource as Source,
};
use tokenmon_ui::kit::breakdown::BreakdownItem;
use tokenmon_ui::panels::grokbot_cursor::*;
use tokenmon_ui::panels::providers::grokbot_breakdown_items;

fn breakdown(unit: BreakdownUnit) -> GrokbotUsageBreakdown {
    GrokbotUsageBreakdown {
        unit,
        period_start: None,
        period_end: Some(at(2026, 7, 20, 0, 0)),
        slices: vec![
            BreakdownSlice {
                source: Source::Automation,
                value: 30.0,
            },
            BreakdownSlice {
                source: Source::User,
                value: 25.0,
            },
        ],
    }
}

fn snap(items: Option<Vec<BreakdownItem>>) -> GrokbotSnapshotInput {
    GrokbotSnapshotInput {
        pool_name: "Weekly".into(),
        used_percent: 55.0,
        has_included_allowance: true,
        resets_at: Some(at(2026, 7, 20, 0, 0)),
        breakdown: items,
        daily: Some(daily(week([2.0; 7]))),
    }
}

/// Percent slices show their share of the pool; the legend names and colors
/// are the Grok Bot app's.
#[test]
fn test_percent_breakdown_items_follow_the_slices() {
    let now = at(2026, 7, 15, 12, 0);
    let items =
        grokbot_breakdown_items(&breakdown(BreakdownUnit::WeeklyLimitPercent), now).unwrap();
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, ["Routines", "You"]);
    assert!(items.iter().all(|i| i.value_text.is_none()));
    assert_eq!(items[0].amount, 30.0);
    assert_eq!(items[0].color, Source::Automation.color());
}

/// On-demand (dollar) slices carry their own `$` text.
#[test]
fn test_dollar_breakdown_items_carry_their_text() {
    let now = at(2026, 7, 15, 12, 0);
    let items = grokbot_breakdown_items(&breakdown(BreakdownUnit::Usd), now).unwrap();
    assert_eq!(items[0].value_text.as_deref(), Some("$30.00"));
    assert_eq!(items[1].value_text.as_deref(), Some("$25.00"));
}

/// The card is hidden once the period has ended, and when nothing was used.
#[test]
fn test_breakdown_hidden_after_the_period_or_without_usage() {
    let ended = at(2026, 7, 20, 0, 0);
    assert!(
        grokbot_breakdown_items(&breakdown(BreakdownUnit::WeeklyLimitPercent), ended).is_none()
    );
    let mut empty = breakdown(BreakdownUnit::WeeklyLimitPercent);
    empty.slices.clear();
    assert!(grokbot_breakdown_items(&empty, at(2026, 7, 15, 12, 0)).is_none());
}

/// The panel builds the card when there are items, and not otherwise.
#[test]
fn test_panel_builds_the_card_only_with_items() {
    let now = at(2026, 7, 15, 12, 0);
    let items = grokbot_breakdown_items(&breakdown(BreakdownUnit::WeeklyLimitPercent), now);
    let with = build_grokbot(
        &GrokbotInput {
            status: status(),
            snapshot: Some(snap(items)),
        },
        &ctx(),
    )
    .usage
    .unwrap();
    let card = with.breakdown.unwrap();
    assert_eq!(card.chips.len(), 2);
    assert_eq!(card.chips[0].value_text, "30%");
    assert!(with.daily.is_some());

    let without = build_grokbot(
        &GrokbotInput {
            status: status(),
            snapshot: Some(snap(None)),
        },
        &ctx(),
    )
    .usage
    .unwrap();
    assert!(without.breakdown.is_none());
}
