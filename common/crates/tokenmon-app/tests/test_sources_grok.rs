//! The Grok typed source, its history access and the production registry's
//! hourly and history seams.

mod common;

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use common::{Env, env, query, t0};
use tokenmon_app::providers::{ProviderDeps, build_registry};
use tokenmon_app::sources::{
    HistoryAccess, NoHistory, SnapshotView, TodayPart, ViewQuery, grok_history, grok_source,
};
use tokenmon_core::settings::Settings;
use tokenmon_core::storage::AppPaths;
use tokenmon_core::usage::Calendar;
use tokenmon_core::{BoxFuture, Clock, ProviderError, ProviderId, ProviderUsagePoller};
use tokenmon_provider_grok::history::GrokHistoryStore;
use tokenmon_provider_grok::models::WeeklyUsageSnapshot;
use tokenmon_provider_grok::poller::{GrokFetch, GrokUsagePoller};
use tokenmon_provider_grok::provider_poller::GrokProviderPoller;

struct Fixed;

impl GrokFetch for Fixed {
    fn fetch(
        &self,
        _cookie: String,
        _email: Option<String>,
    ) -> BoxFuture<'_, Result<WeeklyUsageSnapshot, ProviderError>> {
        Box::pin(async {
            let mut s = WeeklyUsageSnapshot::new(t0(), 42.0);
            s.resets_at = Utc.with_ymd_and_hms(2026, 10, 19, 6, 0, 0).single();
            Ok(s)
        })
    }
}

fn poller(env: &Env) -> Arc<GrokProviderPoller> {
    let clock = env.dyn_clock();
    let inner = Arc::new(GrokUsagePoller::new(
        env.auth(ProviderId::Grok),
        GrokHistoryStore::in_memory(clock.clone()),
        clock,
        Arc::new(Fixed),
        None,
    ));
    let settings = Arc::clone(&env.settings);
    Arc::new(GrokProviderPoller::new(
        inner,
        Arc::new(move || settings.snapshot()),
        |b| b,
    ))
}

/// Grok is empty until polled, then carries the weekly snapshot, its reset
/// and the per-product week.
#[tokio::test]
async fn test_grok_source_goes_from_empty_to_a_snapshot_with_week_bars() {
    let e = env();
    e.sign_in(ProviderId::Grok);
    let p = poller(&e);
    let source = grok_source(&p);
    let clock = e.dyn_clock();
    assert_eq!(
        source(&query(clock.as_ref(), 0, Settings::default())),
        SnapshotView::Grok(None)
    );
    p.refresh_now().await;
    let SnapshotView::Grok(Some(input)) = source(&query(clock.as_ref(), 0, Settings::default()))
    else {
        panic!("snapshot expected after a poll");
    };
    assert_eq!(input.used_percent, 42.0);
    assert_eq!(
        input.resets_at,
        Utc.with_ymd_and_hms(2026, 10, 19, 6, 0, 0).single()
    );
    assert!(input.saved_resets.is_none());
}

/// The history seam over a polled Grok poller lists, flushes and clears the
/// recorded snapshots.
#[tokio::test]
async fn test_grok_history_lists_and_clears_recorded_snapshots() {
    let e = env();
    e.sign_in(ProviderId::Grok);
    let p = poller(&e);
    p.refresh_now().await;
    let history = grok_history(&p);
    assert_eq!(history.snapshots().len(), 1);
    assert!(!history.store_failed());
    assert!(history.is_dirty(), "a fresh row waits for its flush");
    history.flush();
    history.clear();
    assert_eq!(history.snapshots().len(), 0);
}

/// The placeholder history reports nothing and ignores every command.
#[test]
fn test_no_history_is_empty_and_inert() {
    let h = NoHistory;
    assert_eq!(h.snapshots().len(), 0);
    h.clear();
    h.flush();
    assert!(!h.store_failed());
    assert!(!h.is_dirty());
}

/// A query prints its instant and week offset but not its clock.
#[test]
fn test_view_query_debug_shows_now_and_week_offset() {
    let e = env();
    let clock = e.dyn_clock();
    let q: ViewQuery<'_> = query(clock.as_ref(), -2, Settings::default());
    let text = format!("{q:?}");
    assert!(text.contains("ViewQuery"), "{text}");
    assert!(text.contains("week_offset: -2"), "{text}");
}

/// The production registry wires an hourly source for Grok, Claude, Grokbot
/// and OpenCode (each with 24 weights), none for Overview, and a readable
/// history.
#[tokio::test]
async fn test_production_registry_exposes_hourly_sources_and_history() {
    let e = env();
    let dir = tempfile::tempdir().expect("tempdir");
    let paths = AppPaths::new(dir.path());
    let clock: Arc<dyn Clock> = e.dyn_clock();
    let registry = build_registry(&ProviderDeps {
        settings: &e.settings,
        sessions: &e.sessions,
        paths: &paths,
        clock: &clock,
        version: Some("0.0.0-test"),
        home: Some(dir.path().to_path_buf()),
        claude_config_dir: None,
        opencode_env: tokenmon_provider_opencode::db::DataDirEnv::default(),
    })
    .expect("registry builds");
    let cal = Calendar::local(clock.as_ref());
    for provider in [ProviderId::Grok, ProviderId::Claude, ProviderId::Grokbot] {
        let part = registry.hourly(provider).expect("hourly source")(&cal);
        let weights = match part {
            Some(TodayPart::Grok(d) | TodayPart::Claude(d) | TodayPart::Grokbot(d)) => d.weights,
            other => panic!("unexpected part for {provider:?}: {other:?}"),
        };
        assert_eq!(weights.len(), 24);
    }
    assert!(registry.hourly(ProviderId::Overview).is_none());
    assert_eq!(registry.history().snapshots().len(), 0);
}
