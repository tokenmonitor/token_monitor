//! Edge cases across the small decision functions: degenerate rectangles,
//! zero DPI, retry bounds, unknown providers, failed saves and unbuffered
//! observation.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::NaiveDate;
use common::env;
use tokenmon_app::model::{AppModel, CHANGE_COALESCING};
use tokenmon_app::observe::ObservationLoop;
use tokenmon_app::overview::{OverviewStore, hour_columns, record_live};
use tokenmon_app::placement::{AnchorSource, max_panel_height_dip, resolve_anchor};
use tokenmon_app::registry::{ProviderEntry, ProviderRegistry, SessionKey};
use tokenmon_app::sessions::{HostBrowserCleaner, auth_config_of};
use tokenmon_app::sources::TodayPart;
use tokenmon_app::startup::{StartupFailure, retry};
use tokenmon_app::testing::FakePoller;
use tokenmon_app::tray::Rect;
use tokenmon_core::overview::{
    CursorDay, DayHourly, OpenCodeDay, OverviewDaySeries, OverviewSeries,
};
use tokenmon_core::settings::{AppSettings, SettingsFile};
use tokenmon_core::{BrowserStateCleaner, ProviderId};
use tokenmon_platform::webauth::{FakeWebAuthHost, WebAuthHost};
use tokenmon_ui::panels::overview::HourSeries;

fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// A rectangle with no width or no height is not a usable anchor, so the
/// notification area (or the taskbar corner) is used instead.
#[test]
fn test_degenerate_rectangles_are_not_anchors() {
    let monitor = rect(0, 0, 1920, 1080);
    let work = rect(0, 0, 1920, 1040);
    let tray = rect(1700, 1040, 220, 40);
    let no_width = rect(1650, 950, 0, 16);
    let no_height = rect(1650, 950, 16, 0);
    for icon in [no_width, no_height] {
        let (anchor, source) = resolve_anchor(Some(icon), None, Some(tray), monitor, work, 96);
        assert_eq!((anchor, source), (tray, AnchorSource::NotificationArea));
    }
    let (_, source) = resolve_anchor(Some(no_width), None, Some(no_height), monitor, work, 96);
    assert_eq!(source, AnchorSource::Fallback);
}

/// An unknown monitor DPI (zero) is treated as 96.
#[test]
fn test_zero_dpi_is_treated_as_the_base_dpi() {
    assert_eq!(
        max_panel_height_dip(1040, 0),
        max_panel_height_dip(1040, 96)
    );
}

/// The assembly failure tells the user where the log is.
#[test]
fn test_assembly_failure_message_names_the_log_folder() {
    let text = StartupFailure::Assembly("disk full".into())
        .message_for(tokenmon_core::storage::HostOs::Windows);
    assert!(text.contains("set itself up") && text.contains("disk full"));
    assert!(text.contains("TokenMon\\logs"));
}

/// Zero attempts still makes one; the last error is returned.
#[test]
fn test_retry_with_zero_attempts_tries_once() {
    let mut calls = 0;
    let result: Result<(), &str> = retry(
        0,
        |_| panic!("no retry expected"),
        || {
            calls += 1;
            Err("nope")
        },
    );
    assert_eq!(result, Err("nope"));
    assert_eq!(calls, 1);
}

/// A failing attempt is retried with the failure count passed to the hook.
#[test]
fn test_retry_reports_each_failure_before_the_next_attempt() {
    let mut seen = Vec::new();
    let mut calls = 0;
    let result: Result<u32, &str> = retry(
        3,
        |n| seen.push(n),
        || {
            calls += 1;
            if calls < 3 { Err("not yet") } else { Ok(calls) }
        },
    );
    assert_eq!(result, Ok(3));
    assert_eq!(seen, vec![1, 2]);
}

/// Asking for the sharers of a provider that is not registered is empty.
#[test]
fn test_unregistered_provider_shares_its_session_with_nobody() {
    let anchor = Arc::new(0_u8);
    let registry = ProviderRegistry::new(vec![ProviderEntry {
        provider: ProviderId::Grok,
        poller: FakePoller::new(ProviderId::Grok),
        session: SessionKey::of(&anchor),
    }]);
    assert_eq!(registry.sharing_session_with(ProviderId::Grok).len(), 1);
    assert_eq!(registry.sharing_session_with(ProviderId::Claude).len(), 0);
}

/// Clearing browser state for a config no provider owns signs nothing out.
#[test]
fn test_browser_cleaner_ignores_unknown_hosts() {
    let host = Arc::new(FakeWebAuthHost::default());
    let cleaner = HostBrowserCleaner::new(Arc::clone(&host) as Arc<dyn WebAuthHost>);
    let mut config = auth_config_of(ProviderId::Grok).expect("config");
    config.hosts = vec!["example.invalid".into()];
    cleaner.clear_browser_state(&config);
    assert_eq!(host.signed_out.lock().expect("lock").len(), 0);
    cleaner.clear_browser_state(&auth_config_of(ProviderId::Grok).expect("config"));
    assert_eq!(*host.signed_out.lock().expect("lock"), vec!["grok"]);
}

/// Every cookie provider has a writable session; OpenRouter has none.
#[test]
fn test_cookie_session_exists_for_cookie_providers_only() {
    let e = env();
    for p in [
        ProviderId::Grok,
        ProviderId::Cursor,
        ProviderId::Claude,
        ProviderId::ChatGpt,
        ProviderId::OpenCode,
        ProviderId::Grokbot,
    ] {
        assert!(e.sessions.cookie_session(p).is_some(), "{p}");
    }
    assert!(e.sessions.cookie_session(ProviderId::OpenRouter).is_none());
}

/// A history that cannot be saved is still recorded in memory; the save
/// failure is logged, not fatal.
#[test]
fn test_overview_store_survives_a_failed_save() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("gone");
    std::fs::create_dir(&path).expect("mkdir");
    let store = OverviewStore::open(&path);
    std::fs::remove_dir(&path).expect("rmdir");
    let mut w = vec![0.0; 24];
    w[2] = 1.0;
    let series = OverviewDaySeries::default().with_weights(OverviewSeries::Grok, w);
    let day = NaiveDate::from_ymd_opt(2026, 7, 10).expect("date");
    assert!(store.record(&series, day));
    assert!(store.series(day).is_some());
}

/// Non-finite and non-positive weights draw no activity.
#[test]
fn test_hour_columns_skip_non_finite_and_non_positive_weights() {
    let mut weights = vec![0.0; 24];
    weights[0] = f64::NAN;
    weights[1] = f64::INFINITY;
    weights[2] = -1.0;
    weights[3] = 1.0;
    let series = OverviewDaySeries::default().with_weights(OverviewSeries::Claude, weights);
    let columns = hour_columns(&series);
    assert_eq!(
        (0..4).map(|h| columns[h].len()).collect::<Vec<_>>(),
        vec![0, 0, 0, 1]
    );
}

/// A series shorter than 24 hours simply has no activity in the missing hours.
#[test]
fn test_hour_columns_tolerate_short_series() {
    let series = OverviewDaySeries::default().with_weights(OverviewSeries::Claude, vec![1.0; 3]);
    let columns = hour_columns(&series);
    assert_eq!(columns.len(), 24);
    assert_eq!(columns[2].len(), 1);
    assert_eq!(columns[3].len(), 0);
}

/// Every provider's hourly part reaches the stored Overview day.
#[test]
fn test_record_live_merges_every_provider_part() {
    let e = env();
    for p in [
        ProviderId::Grok,
        ProviderId::Cursor,
        ProviderId::Claude,
        ProviderId::OpenCode,
        ProviderId::Grokbot,
    ] {
        if p != ProviderId::OpenCode {
            e.sign_in(p);
        }
    }
    let day = NaiveDate::from_ymd_opt(2026, 10, 14).expect("date");
    let hour = |v: f64| {
        let mut w = vec![0.0; 24];
        w[4] = v;
        w
    };
    let tokens = |n: i64| {
        let mut t = vec![0; 24];
        t[4] = n;
        t
    };
    let dh = move |v| DayHourly {
        day,
        weights: hour(v),
    };
    let registry = ProviderRegistry::new(Vec::new())
        .with_hourly(
            ProviderId::Grok,
            Arc::new(move |_| Some(TodayPart::Grok(dh(1.0)))),
        )
        .with_hourly(
            ProviderId::Claude,
            Arc::new(move |_| Some(TodayPart::Claude(dh(2.0)))),
        )
        .with_hourly(
            ProviderId::Grokbot,
            Arc::new(move |_| Some(TodayPart::Grokbot(dh(3.0)))),
        )
        .with_hourly(
            ProviderId::Cursor,
            Arc::new(move |_| {
                Some(TodayPart::Cursor(CursorDay {
                    day,
                    quota_weights: hour(4.0),
                    hour_tokens: tokens(40),
                }))
            }),
        )
        .with_hourly(
            ProviderId::OpenCode,
            Arc::new(move |_| {
                Some(TodayPart::OpenCode(OpenCodeDay {
                    day,
                    go_weights: hour(5.0),
                    zen_weights: hour(6.0),
                    go_tokens: tokens(50),
                    zen_tokens: tokens(60),
                }))
            }),
        );
    let dir = tempfile::tempdir().expect("tempdir");
    let store = OverviewStore::open(dir.path());
    let settings = e.settings.snapshot();
    let changed = record_live(
        &store,
        &registry,
        &settings,
        &e.sessions,
        common::t0(),
        e.clock.as_ref(),
    );
    assert!(changed);
    let series = store.series(day).expect("today stored");
    let columns = hour_columns(&series);
    let kinds: Vec<HourSeries> = columns[4].iter().map(|a| a.series).collect();
    for expected in [
        HourSeries::Grok,
        HourSeries::Claude,
        HourSeries::Grokbot,
        HourSeries::Cursor,
        HourSeries::OpenCodeGo,
        HourSeries::OpenCodeZen,
    ] {
        assert!(kinds.contains(&expected), "{expected:?} in {kinds:?}");
    }
    assert!(!record_live(
        &store,
        &registry,
        &settings,
        &e.sessions,
        common::t0(),
        e.clock.as_ref(),
    ));
}

/// Pinning a graph for a provider the registry does not hold refreshes
/// nobody and does not disturb the watcher.
#[tokio::test(start_paused = true)]
async fn test_new_graph_for_an_unregistered_provider_is_ignored() {
    let anchor = Arc::new(0_u8);
    let grok = FakePoller::new(ProviderId::Grok);
    let settings = Arc::new(AppSettings::new(Arc::new(SettingsFile::in_memory())));
    let model = AppModel::new(
        ProviderRegistry::new(vec![ProviderEntry {
            provider: ProviderId::Grok,
            poller: grok.clone(),
            session: SessionKey::of(&anchor),
        }]),
        Arc::clone(&settings),
        tokio::runtime::Handle::current(),
    );
    let watch = model.watch_settings();
    settings.set_menu_bar_graph(ProviderId::Cursor, true);
    tokio::time::sleep(CHANGE_COALESCING * 3).await;
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert_eq!(grok.refreshes(), 0);
    assert!(!watch.is_finished());
    watch.abort();
}

/// With no coalescing window every distinct value reaches the handler.
#[tokio::test]
async fn test_observation_without_coalescing_handles_each_change() {
    let (tx, rx) = tokio::sync::watch::channel(0_u32);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let _loop = ObservationLoop::start(
        &tokio::runtime::Handle::current(),
        rx,
        Duration::ZERO,
        false,
        |v: &u32| *v,
        move |v| sink.lock().expect("lock").push(v),
    );
    for v in [1_u32, 2] {
        tx.send(v).expect("receiver alive");
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }
    assert_eq!(*seen.lock().expect("lock"), vec![1, 2]);
}
