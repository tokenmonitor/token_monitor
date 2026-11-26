//! Edge cases of the local estimate and snapshot merging: anonymous
//! sessions, token-only models, degenerate limits and the console/local merge.
#![allow(
    clippy::unwrap_used,
    clippy::assert_is_empty,
    clippy::unnecessary_wraps
)]

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use tokenmon_core::usage::Calendar;
use tokenmon_provider_opencode::db::{MessageRow, Scan};
use tokenmon_provider_opencode::pricing::Tokens;
use tokenmon_provider_opencode::snapshot::{ModelUsage, OpenCodeSnapshot, WindowKind, WindowUsage};
use tokenmon_provider_opencode::stats::{
    build_day_hourly_usage, daily_spends_by_day, model_usage, overview_hour_token_counts,
    overview_quota_hour_weights, snapshot,
};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, 12, 0, 0).unwrap()
}

fn row(session: &str, provider: &str, model: &str, cost: f64, tokens: Tokens) -> MessageRow {
    MessageRow {
        date: now() - chrono::Duration::hours(1),
        session_id: session.to_owned(),
        provider_id: provider.to_owned(),
        model_id: model.to_owned(),
        cost_usd: cost,
        tokens,
    }
}

const NONE: Tokens = Tokens {
    input: 0,
    output: 0,
    cache_read: 0,
    cache_write: 0,
};

fn model(id: &str) -> ModelUsage {
    ModelUsage {
        provider_id: "opencode-go".to_owned(),
        model_id: id.to_owned(),
        cost_usd: 1.0,
        input_tokens: 1,
        output_tokens: 1,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        session_count: 1,
        percent_of_window: 100.0,
        is_cost_estimated: false,
        is_cost_unpriced: false,
    }
}

/// Rows without a session id each count as their own session, both in the
/// windows and per model.
#[test]
fn test_anonymous_rows_count_as_separate_sessions() {
    let rows = vec![
        row("", "opencode-go", "m", 1.0, NONE),
        row("", "opencode-go", "m", 1.0, NONE),
        row("s1", "opencode-go", "m", 1.0, NONE),
        row("s1", "opencode-go", "m", 1.0, NONE),
    ];
    let scan = Scan {
        subscribed_at: None,
        rows: rows.clone(),
    };
    let snap = snapshot(&scan, now());
    let rolling = snap.window(WindowKind::Rolling5h).unwrap();
    assert_eq!(rolling.session_count, 3);
    let refs: Vec<&MessageRow> = rows.iter().collect();
    let models = model_usage(&refs);
    assert_eq!(models.len(), 1);
    assert_eq!(
        models[0].session_count, 1,
        "the named session overwrites the count"
    );
}

/// A model with no cost but any single token kind is listed; one with
/// nothing at all is dropped.
#[test]
fn test_models_with_only_one_token_kind_are_kept() {
    let kinds = [
        Tokens { output: 5, ..NONE },
        Tokens {
            cache_read: 5,
            ..NONE
        },
        Tokens {
            cache_write: 5,
            ..NONE
        },
        Tokens { input: 5, ..NONE },
    ];
    for (i, t) in kinds.iter().enumerate() {
        let r = row("s", "opencode", &format!("free-{i}"), 0.0, *t);
        let models = model_usage(&[&r]);
        assert_eq!(models.len(), 1, "kind {i}");
        assert_eq!(models[0].percent_of_window, 0.0);
    }
    let empty = row("s", "opencode", "idle", 0.0, NONE);
    assert!(model_usage(&[&empty]).is_empty());
}

/// Rows that cost nothing are left out of the daily spends, the rest are
/// summed per local day.
#[test]
fn test_daily_spends_skip_non_positive_costs() {
    let free = row("s", "opencode-go", "m", 0.0, NONE);
    let paid = row("s", "opencode-go", "m", 2.0, NONE);
    let paid2 = row("s", "opencode-go", "m", 1.5, NONE);
    let spends = daily_spends_by_day(&[&free, &paid, &paid2], FixedOffset::east_opt(0).unwrap());
    assert_eq!(spends.len(), 1);
    assert!((spends.values().next().unwrap() - 3.5).abs() < 1e-9);
}

/// Hour segments with equal cost and message counts are ordered by provider
/// then model, and the overview series split Go from Zen.
#[test]
fn test_hourly_ties_sort_by_provider_and_model_and_series_split_providers() {
    let rows = vec![
        row(
            "s",
            "opencode-go",
            "zeta",
            1.0,
            Tokens { input: 10, ..NONE },
        ),
        row(
            "s",
            "opencode-go",
            "alpha",
            1.0,
            Tokens { input: 20, ..NONE },
        ),
        row("s", "opencode", "zen", 1.0, Tokens { input: 30, ..NONE }),
        row("s", "other", "x", 1.0, Tokens { input: 40, ..NONE }),
    ];
    let cal = Calendar::utc();
    let day = build_day_hourly_usage(&rows, now(), &cal, 2);
    let hour = day.hours.iter().find(|h| h.message_count > 0).unwrap();
    let order: Vec<(&str, &str)> = hour
        .segments
        .iter()
        .map(|s| (s.provider_id.as_str(), s.model_id.as_str()))
        .collect();
    assert_eq!(
        order,
        [
            ("opencode", "zen"),
            ("opencode-go", "alpha"),
            ("opencode-go", "zeta"),
            ("other", "x")
        ]
    );
    assert_eq!(day.legend.len(), 2);
    let tokens = overview_hour_token_counts(&day);
    assert_eq!(tokens.go.iter().sum::<f64>(), 30.0);
    assert_eq!(tokens.zen.iter().sum::<f64>(), 30.0);
}

/// Quota weights are all zero when the monthly limit leaves no positive
/// weekly share, and proportional otherwise.
#[test]
fn test_quota_weights_need_a_positive_limit() {
    let rows = vec![row("s", "opencode-go", "m", 3.0, NONE)];
    let day = build_day_hourly_usage(&rows, now(), &Calendar::utc(), 8);
    for bad in [0.0, -5.0, f64::INFINITY, f64::NAN] {
        let w = overview_quota_hour_weights(&day, bad);
        assert!(w.go.iter().chain(w.zen.iter()).all(|v| *v == 0.0), "{bad}");
    }
    let w = overview_quota_hour_weights(&day, 60.0);
    assert!(w.go.iter().sum::<f64>() > 0.0);
}

/// The local-stats flag is true for models, tokens or spend alone.
#[test]
fn test_has_local_stats_each_signal() {
    assert!(!OpenCodeSnapshot::default().has_local_stats());
    let with_model = OpenCodeSnapshot {
        models: vec![model("m")],
        ..OpenCodeSnapshot::default()
    };
    assert!(with_model.has_local_stats());
    let with_tokens = OpenCodeSnapshot {
        monthly_tokens: 1,
        ..OpenCodeSnapshot::default()
    };
    assert!(with_tokens.has_local_stats());
    let with_usd = OpenCodeSnapshot {
        monthly_estimated_usd: 0.5,
        ..OpenCodeSnapshot::default()
    };
    assert!(with_usd.has_local_stats());
}

fn console() -> OpenCodeSnapshot {
    OpenCodeSnapshot {
        windows: vec![WindowUsage {
            kind: WindowKind::Monthly,
            used_usd: 6.0,
            limit_usd: 60.0,
            resets_at: None,
            session_count: 0,
        }],
        models: vec![model("console-model")],
        is_estimated: false,
        monthly_tokens: 100,
        monthly_estimated_usd: 9.0,
        monthly_input_tokens: 60,
        monthly_output_tokens: 40,
    }
}

/// Merging keeps the console windows and takes whatever local data exists:
/// nothing local changes nothing, models only, totals only, or both.
#[test]
fn test_merge_with_local_takes_only_what_local_has() {
    let base = console();
    assert_eq!(base.clone().merged_with_local(None), base);

    let empty_local = OpenCodeSnapshot::default();
    assert_eq!(base.clone().merged_with_local(Some(&empty_local)), base);

    let models_only = OpenCodeSnapshot {
        models: vec![model("local-model")],
        ..OpenCodeSnapshot::default()
    };
    let merged = base.clone().merged_with_local(Some(&models_only));
    assert_eq!(merged.models[0].model_id, "local-model");
    assert_eq!(merged.monthly_tokens, 100);
    assert_eq!(merged.windows, base.windows);

    let totals_only = OpenCodeSnapshot {
        monthly_tokens: 7,
        monthly_estimated_usd: 0.7,
        monthly_input_tokens: 3,
        monthly_output_tokens: 4,
        ..OpenCodeSnapshot::default()
    };
    let merged = base.clone().merged_with_local(Some(&totals_only));
    assert_eq!(merged.models[0].model_id, "console-model");
    assert_eq!(
        (
            merged.monthly_tokens,
            merged.monthly_input_tokens,
            merged.monthly_output_tokens
        ),
        (7, 3, 4)
    );
    assert!((merged.monthly_estimated_usd - 0.7).abs() < 1e-12);
    assert!(!merged.is_estimated);
}
