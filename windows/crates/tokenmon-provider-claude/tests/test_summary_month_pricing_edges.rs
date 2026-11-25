//! Edge cases of the money formatting, ranking shares, month coverage and
//! model naming.
#![allow(clippy::unwrap_used)]

use chrono::{FixedOffset, NaiveDate, TimeZone, Utc};
use tokenmon_provider_claude::month::{ClaudeMonthCoverage, ClaudeUsageMonth};
use tokenmon_provider_claude::pricing::priced_model_ids;
use tokenmon_provider_claude::summary::format_usd;
use tokenmon_provider_claude::{
    ClaudeCodeUsageSummary, DailyTokens, DayKey, ModelTokens, UsageTotals, base_model_id,
    display_name, rates,
};

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
}

fn tokens(input: i64) -> ModelTokens {
    ModelTokens {
        input_tokens: input,
        response_count: 1,
        ..ModelTokens::default()
    }
}

/// Dollar amounts are grouped by thousands, rounded to cents, signed only
/// when the rounded amount is non-zero, and non-finite values render as zero.
#[test]
fn test_format_usd_groups_rounds_signs_and_survives_non_finite() {
    assert_eq!(format_usd(1_234_567.891), "$1,234,567.89");
    assert_eq!(format_usd(0.0), "$0.00");
    assert_eq!(format_usd(-12.5), "-$12.50");
    assert_eq!(format_usd(-0.001), "$0.00");
    assert_eq!(format_usd(f64::NAN), "$0.00");
    assert_eq!(format_usd(f64::INFINITY), "$0.00");
}

/// The spend label marks a lower bound with `+` only when something is
/// unpriced.
#[test]
fn test_spend_label_marks_unpriced_lower_bounds() {
    let exact = UsageTotals {
        tokens: 1,
        cost_usd: 3.0,
        has_unpriced: false,
    };
    assert_eq!(exact.spend_label(), "$3.00");
    let bound = UsageTotals {
        has_unpriced: true,
        ..exact
    };
    assert_eq!(bound.spend_label(), "$3.00+");
}

/// With only unpriced models, rows share the month by tokens and flag the
/// total as a lower bound; a model with no tokens is left out.
#[test]
fn test_unpriced_models_share_the_month_by_tokens() {
    let mut days = DailyTokens::new();
    let key = DayKey::for_date(day(2026, 10, 3));
    let models = days.entry(key).or_default();
    models.insert("mystery-a".to_owned(), tokens(300));
    models.insert("mystery-b".to_owned(), tokens(100));
    models.insert("mystery-c".to_owned(), ModelTokens::default());
    let now = Utc.with_ymd_and_hms(2026, 10, 20, 0, 0, 0).unwrap();
    let summary = ClaudeCodeUsageSummary::make(&days, now);
    assert!(summary.month.has_unpriced);
    assert_eq!(summary.month_models.len(), 2);
    assert_eq!(summary.month_models[0].model_id, "mystery-a");
    assert!((summary.month_models[0].share_percent - 75.0).abs() < 1e-9);
    assert!((summary.month_models[1].share_percent - 25.0).abs() < 1e-9);
    assert!(summary.month_models.iter().all(|r| r.is_unpriced));
}

/// A month the ledger starts after is uncovered but remembers when
/// recording began; a month the ledger starts inside is partial.
#[test]
fn test_month_coverage_for_months_before_and_around_the_first_saved_day() {
    let offset = FixedOffset::east_opt(0).expect("offset");
    let sep =
        ClaudeUsageMonth::containing(Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap(), offset);
    let oct =
        ClaudeUsageMonth::containing(Utc.with_ymd_and_hms(2026, 10, 15, 0, 0, 0).unwrap(), offset);
    let first = DayKey::for_date(day(2026, 10, 9));
    assert_eq!(
        ClaudeMonthCoverage::of(sep, Some(&first)),
        ClaudeMonthCoverage::None {
            recorded_since: Some(day(2026, 10, 9))
        }
    );
    assert_eq!(
        ClaudeMonthCoverage::of(oct, Some(&first)),
        ClaudeMonthCoverage::Partial {
            since: day(2026, 10, 9)
        }
    );
}

/// Month accessors report the first day and offset, and the span runs to the
/// month's last day unless `since` already is the last day.
#[test]
fn test_month_span_first_day_and_offset() {
    let offset = FixedOffset::west_opt(5 * 3600).expect("offset");
    let oct = ClaudeUsageMonth::containing(
        Utc.with_ymd_and_hms(2026, 10, 20, 12, 0, 0).unwrap(),
        offset,
    );
    assert_eq!(oct.first_day(), day(2026, 10, 1));
    assert_eq!(oct.offset(), offset);
    assert_eq!(oct.span(day(2026, 10, 9)), "Oct 9–31");
    assert_eq!(oct.span(day(2026, 10, 31)), "Oct 31");
    let feb =
        ClaudeUsageMonth::containing(Utc.with_ymd_and_hms(2028, 2, 20, 12, 0, 0).unwrap(), offset);
    assert_eq!(feb.span(day(2028, 2, 1)), "Feb 1–29");
}

/// Snapshot suffixes are stripped only when they are exactly eight digits.
#[test]
fn test_base_model_id_strips_only_eight_digit_suffixes() {
    assert_eq!(
        base_model_id("claude-haiku-4-5-20251001"),
        "claude-haiku-4-5"
    );
    assert_eq!(
        base_model_id("claude-haiku-4-5-2025100a"),
        "claude-haiku-4-5-2025100a"
    );
    assert_eq!(
        base_model_id("claude-haiku-4-5-2025"),
        "claude-haiku-4-5-2025"
    );
    assert_eq!(base_model_id("nodash"), "nodash");
}

/// Every priced id resolves to rates, and unknown ids do not.
#[test]
fn test_every_priced_model_id_has_rates() {
    let ids: Vec<&str> = priced_model_ids().collect();
    assert!(ids.len() > 10);
    for id in ids {
        assert!(rates(id).is_some(), "{id}");
    }
    assert!(rates("claude-nope-1").is_none());
}

/// Display names apply the brand table, numeric joining, GPT/GLM version
/// hyphens, the Muse Spark special case and the empty-id fallback.
#[test]
fn test_display_name_applies_brand_and_version_rules() {
    assert_eq!(display_name("gpt-5.4-mini"), "GPT-5.4 Mini");
    assert_eq!(display_name("glm-4.6"), "GLM-4.6");
    assert_eq!(display_name("gpt-oss-120b"), "GPT Oss 120b");
    assert_eq!(display_name("minimax-m2"), "MiniMax M2");
    assert_eq!(display_name("deepseek-v3.2"), "DeepSeek V3.2");
    assert_eq!(display_name("mimo-v2"), "MiMo V2");
    assert_eq!(display_name("hy3-preview"), "HY3 Preview");
    assert_eq!(display_name("claude-muse-spark-2"), "Muse Spark");
    assert_eq!(display_name("---"), "---");
    assert_eq!(display_name("1-2-3"), "1.2.3");
}
