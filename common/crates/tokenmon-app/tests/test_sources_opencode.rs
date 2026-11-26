//! The OpenCode typed source and hourly source over a poller fed by a scripted
//! console and an injected local scan.

mod common;

use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use common::{Env, env, query, t0};
use tokenmon_app::sources::{SnapshotView, TodayPart, opencode_hourly, opencode_source};
use tokenmon_core::settings::{PanelSection, Settings};
use tokenmon_core::usage::Calendar;
use tokenmon_core::{ProviderError, ProviderId, ProviderUsagePoller};
use tokenmon_provider_opencode::budget::ScanFn;
use tokenmon_provider_opencode::db::{DbError, MessageRow, Scan};
use tokenmon_provider_opencode::poller::{FetchConsoleFn, OpenCodePollerDeps, OpenCodeUsagePoller};
use tokenmon_provider_opencode::pricing::Tokens;
use tokenmon_provider_opencode::snapshot::{OpenCodeSnapshot, WindowKind, WindowUsage};

fn window(kind: WindowKind, used: f64, limit: f64) -> WindowUsage {
    WindowUsage {
        kind,
        used_usd: used,
        limit_usd: limit,
        resets_at: Utc.with_ymd_and_hms(2026, 11, 1, 0, 0, 0).single(),
        session_count: 1,
    }
}

fn scan() -> Scan {
    Scan {
        subscribed_at: Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).single(),
        rows: vec![MessageRow {
            date: t0() - Duration::hours(1),
            session_id: "s1".into(),
            provider_id: "opencode-go".into(),
            model_id: "kimi-k2.5".into(),
            cost_usd: 2.0,
            tokens: Tokens {
                input: 100,
                output: 10,
                cache_read: 0,
                cache_write: 0,
            },
        }],
    }
}

fn poller(env: &Env, windows: Vec<WindowUsage>, scan_ok: bool) -> Arc<OpenCodeUsagePoller> {
    let snapshot = OpenCodeSnapshot {
        windows,
        ..OpenCodeSnapshot::default()
    };
    let fetch: Arc<FetchConsoleFn> = Arc::new(move |_cookie, _org| {
        let snapshot = snapshot.clone();
        Box::pin(async move { Ok::<_, ProviderError>((snapshot, "wrk_1".to_owned())) })
    });
    let scan = Arc::new(scan());
    let load: Arc<ScanFn> = Arc::new(move |_since| {
        if scan_ok {
            Ok(Arc::clone(&scan))
        } else {
            Err(DbError::Missing("opencode.db".into()))
        }
    });
    OpenCodeUsagePoller::new(OpenCodePollerDeps {
        settings: Arc::clone(&env.settings),
        auth: env.auth(ProviderId::OpenCode),
        clock: env.dyn_clock(),
        fetch_console: fetch,
        load_scan: load,
    })
}

/// Before the first poll the OpenCode view is empty.
#[tokio::test]
async fn test_opencode_source_is_empty_before_the_first_poll() {
    let e = env();
    let source = opencode_source(&poller(&e, vec![], true));
    let clock = e.dyn_clock();
    assert_eq!(
        source(&query(clock.as_ref(), 0, Settings::default())),
        SnapshotView::OpenCode(None)
    );
}

/// A monthly window drives the chart; the models-expanded flag follows the
/// settings.
#[tokio::test]
async fn test_opencode_source_uses_the_monthly_window_for_the_chart() {
    let e = env();
    e.sign_in(ProviderId::OpenCode);
    let p = poller(&e, vec![window(WindowKind::Monthly, 30.0, 60.0)], true);
    p.refresh_now().await;
    let source = opencode_source(&p);
    let clock = e.dyn_clock();
    let SnapshotView::OpenCode(Some(input)) =
        source(&query(clock.as_ref(), 0, Settings::default()))
    else {
        panic!("snapshot expected");
    };
    assert!(input.daily.is_some(), "monthly bars are charted");
    let collapsed = Settings::default().with_expanded(PanelSection::OpenCodeModels, false);
    let SnapshotView::OpenCode(Some(collapsed_input)) =
        source(&query(clock.as_ref(), 0, collapsed))
    else {
        panic!("snapshot expected");
    };
    assert_ne!(input.models_expanded, collapsed_input.models_expanded);
}

/// With no monthly window the weekly one supplies the used percent, and with
/// neither the chart is paced from zero.
#[tokio::test]
async fn test_opencode_source_falls_back_to_weekly_then_zero() {
    let e = env();
    e.sign_in(ProviderId::OpenCode);
    let clock = e.dyn_clock();
    for windows in [
        vec![window(WindowKind::Weekly, 5.0, 10.0)],
        vec![window(WindowKind::Rolling5h, 1.0, 10.0)],
    ] {
        let p = poller(&e, windows, true);
        p.refresh_now().await;
        let source = opencode_source(&p);
        let view = source(&query(clock.as_ref(), 0, Settings::default()));
        assert!(matches!(view, SnapshotView::OpenCode(Some(_))), "{view:?}");
    }
}

/// Today's hours come from the local scan, scaled by the monthly limit.
#[tokio::test]
async fn test_opencode_hourly_reports_today_scaled_by_the_monthly_limit() {
    let e = env();
    e.sign_in(ProviderId::OpenCode);
    let clock = e.dyn_clock();
    let cal = Calendar::local(clock.as_ref());

    let with_monthly = poller(&e, vec![window(WindowKind::Monthly, 30.0, 120.0)], true);
    with_monthly.refresh_now().await;
    let Some(TodayPart::OpenCode(a)) = opencode_hourly(&with_monthly, e.dyn_clock())(&cal) else {
        panic!("opencode part");
    };

    let without = poller(&e, vec![], true);
    without.refresh_now().await;
    let Some(TodayPart::OpenCode(b)) = opencode_hourly(&without, e.dyn_clock())(&cal) else {
        panic!("opencode part");
    };
    let total = |d: &tokenmon_core::overview::OpenCodeDay| d.go_weights.iter().sum::<f64>();
    assert!(total(&a) > 0.0 && total(&b) > 0.0);
    assert!(
        total(&b) > total(&a),
        "the default 60 USD limit weighs the same spend more than a 120 USD one"
    );
}

/// With no readable local data there are no hours to report.
#[tokio::test]
async fn test_opencode_hourly_is_none_without_local_data() {
    let e = env();
    let p = poller(&e, vec![], false);
    let clock = e.dyn_clock();
    let cal = Calendar::local(clock.as_ref());
    assert!(opencode_hourly(&p, e.dyn_clock())(&cal).is_none());
}
