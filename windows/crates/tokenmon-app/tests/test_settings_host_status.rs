//! Settings backend changes and the host's refresh status, sign-in and
//! sign-out, driven over scripted pollers and platform fakes.

mod common;

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Utc};
use common::{env, t0};
use tokenmon_app::model::AppModel;
use tokenmon_app::poller::ManagedPoller;
use tokenmon_app::registry::{ProviderEntry, ProviderRegistry, SessionKey};
use tokenmon_app::sessions::{HostBrowserCleaner, Sessions};
use tokenmon_app::settings_host::{AppHost, AppSettingsBackend, HostParts};
use tokenmon_app::signin::SignInService;
use tokenmon_app::sources::NoHistory;
use tokenmon_app::updates::UpdateService;
use tokenmon_core::settings::{AppSettings, SettingsFile, ThemePreference};
use tokenmon_core::update::UrlOpener;
use tokenmon_core::{
    BoxFuture, BrowserStateCleaner, Clock, ProviderError, ProviderErrorContext, ProviderId,
    ProviderUsagePoller,
};
use tokenmon_platform::secrets::{FakeEncryptor, Vault};
use tokenmon_platform::shell::{DesktopVerb, MemoryRegistry, RunKeyLoginItem};
use tokenmon_platform::webauth::{FakeWebAuthHost, WebAuthHost};
use tokenmon_ui::settings::{
    ProductCategory, RefreshStatus, SettingsBackend, SettingsChange, SettingsHost,
};

/// A poller that reports a fixed refresh time and error.
struct Stamped {
    provider: ProviderId,
    refreshed: Option<DateTime<Utc>>,
    error: Option<&'static str>,
}

impl ProviderUsagePoller for Stamped {
    fn provider(&self) -> ProviderId {
        self.provider
    }
    fn set_menu_open(&self, _open: bool) {}
    fn last_refreshed_at(&self) -> Option<DateTime<Utc>> {
        self.refreshed
    }
    fn last_error(&self) -> Option<ProviderError> {
        self.error
            .map(|e| ProviderError::Network(ProviderErrorContext::Grok, e.into()))
    }
    fn headline_used_percent(&self) -> Option<f64> {
        None
    }
    fn headline_resets_at(&self) -> Option<DateTime<Utc>> {
        None
    }
    fn alert_account(&self) -> Option<String> {
        None
    }
    fn start(&self) {}
    fn stop(&self) {}
    fn refresh_now(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
    fn clear_snapshot(&self) {}
}

impl ManagedPoller for Stamped {
    fn wake(&self) {}
    fn pause(&self) {}
    fn resume(&self) {}
    fn refresh_now_if_older_than(&self, _age: Duration) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }
}

struct NoOpener;
impl UrlOpener for NoOpener {
    fn open(&self, _url: &str) {}
}

struct Rig {
    host: AppHost,
    settings: Arc<AppSettings>,
    web: Arc<FakeWebAuthHost>,
    _dir: tempfile::TempDir,
}

/// Grok refreshed at `t0` with an error, Cursor refreshed at `t0`, the rest
/// never refreshed.
fn rig() -> Rig {
    rig_with(true)
}

/// As [`rig`], but with `refreshed` false no poller has ever refreshed.
fn rig_with(refreshed_pollers: bool) -> Rig {
    let dir = tempfile::tempdir().expect("tempdir");
    let web = Arc::new(FakeWebAuthHost::default());
    let vault = Arc::new(Vault::new(dir.path(), Arc::new(FakeEncryptor::default())));
    let clock: Arc<dyn Clock> = env().clock;
    let cleaner: Arc<dyn BrowserStateCleaner> = Arc::new(HostBrowserCleaner::new(
        Arc::clone(&web) as Arc<dyn WebAuthHost>
    ));
    let sessions = Arc::new(Sessions::open(dir.path(), &vault, &clock, &cleaner));
    let settings = Arc::new(AppSettings::new(Arc::new(SettingsFile::in_memory())));
    let anchors: Vec<_> = ProviderId::USAGE_PROVIDERS
        .iter()
        .map(|p| (*p, Arc::new(0_u8)))
        .collect();
    let entries = anchors
        .iter()
        .map(|(provider, anchor)| ProviderEntry {
            provider: *provider,
            poller: Arc::new(Stamped {
                provider: *provider,
                refreshed: (refreshed_pollers
                    && matches!(provider, ProviderId::Grok | ProviderId::Cursor))
                .then(t0),
                error: (*provider == ProviderId::Grok).then_some("offline"),
            }),
            session: SessionKey::of(anchor),
        })
        .collect();
    std::mem::forget(anchors);
    let model = AppModel::new(
        ProviderRegistry::new(entries),
        Arc::clone(&settings),
        tokio::runtime::Handle::current(),
    );
    let signin = Arc::new(SignInService::new(
        Arc::clone(&web) as Arc<dyn WebAuthHost>,
        Arc::clone(&sessions),
        Arc::new(|_| {}),
    ));
    let updates = Arc::new(
        UpdateService::production(
            &settings,
            "1.2.3",
            "TokenMon.exe".into(),
            Arc::new(NoOpener),
        )
        .expect("updates"),
    );
    let registry = Arc::new(MemoryRegistry::new());
    let host = AppHost::new(HostParts {
        settings: Arc::clone(&settings),
        model,
        sessions,
        signin,
        updates,
        login_item: Arc::new(RunKeyLoginItem::new(registry.clone(), "TokenMon.exe")),
        desktop_verb: Arc::new(DesktopVerb::new(registry, "TokenMon.exe")),
        history: Arc::new(NoHistory),
        version: "1.2.3".to_owned(),
        tz: Arc::new(|_| FixedOffset::west_opt(6 * 3600).expect("offset")),
        handle: tokio::runtime::Handle::current(),
        quit: Arc::new(|| {}),
    });
    Rig {
        host,
        settings,
        web,
        _dir: dir,
    }
}

/// Every setting the window can change reaches the store and reads back.
#[tokio::test]
async fn test_backend_applies_every_kind_of_change() {
    let r = rig();
    let backend = AppSettingsBackend::new(Arc::clone(&r.settings));
    let before = backend.snapshot();
    backend.apply(SettingsChange::ShowCategoriesInMenuBar(
        !before.show_categories_in_menu_bar,
    ));
    backend.apply(SettingsChange::ShowSelectedProviderOnly(
        !before.show_selected_provider_only,
    ));
    backend.apply(SettingsChange::ChecksForUpdates(!before.checks_for_updates));
    backend.apply(SettingsChange::ThemePreference(ThemePreference::Dark));
    backend.apply(SettingsChange::IdlePollSeconds(900));
    let after = backend.snapshot();
    assert_ne!(
        after.show_categories_in_menu_bar,
        before.show_categories_in_menu_bar
    );
    assert_ne!(
        after.show_selected_provider_only,
        before.show_selected_provider_only
    );
    assert_ne!(after.checks_for_updates, before.checks_for_updates);
    assert_eq!(after.theme_preference, ThemePreference::Dark);
    assert_eq!(after.idle_poll_seconds, 900);
}

/// Each Grok product category can be hidden and shown again.
#[tokio::test]
async fn test_backend_toggles_every_product_category() {
    let r = rig();
    let backend = AppSettingsBackend::new(Arc::clone(&r.settings));
    let all = [
        ProductCategory::Build,
        ProductCategory::Api,
        ProductCategory::Chat,
        ProductCategory::Imagine,
        ProductCategory::Voice,
        ProductCategory::Other,
    ];
    for category in all {
        backend.apply(SettingsChange::ProductVisible(category, true));
        assert!(backend.snapshot().visible_products.contains(&category));
        backend.apply(SettingsChange::ProductVisible(category, false));
        assert!(!backend.snapshot().visible_products.contains(&category));
    }
}

/// With the Overview selected the window lists each refreshed provider; with
/// none refreshed it shows nothing.
#[tokio::test]
async fn test_refresh_status_per_provider_on_the_overview() {
    let r = rig();
    r.settings.set_selected_provider(ProviderId::Overview);
    let RefreshStatus::PerProvider(lines) = r.host.status().refresh else {
        panic!("per-provider lines expected");
    };
    let providers: Vec<_> = lines.iter().map(|(p, _)| *p).collect();
    assert!(providers.contains(&ProviderId::Grok));
    assert!(providers.contains(&ProviderId::Cursor));
    assert!(!providers.contains(&ProviderId::Claude));
    assert!(lines[0].1.contains("Oct 14, 2026 at 9:00 AM"), "{lines:?}");
}

/// With a provider selected the window shows its last refresh and error, or
/// nothing when it never refreshed.
#[tokio::test]
async fn test_refresh_status_for_a_selected_provider() {
    let r = rig();
    r.settings.set_selected_provider(ProviderId::Grok);
    match r.host.status().refresh {
        RefreshStatus::Single {
            last_refresh,
            last_error,
        } => {
            assert!(last_refresh.contains("Oct 14, 2026"), "{last_refresh}");
            assert!(last_error.expect("error shown").contains("offline"));
        }
        other => panic!("single status expected, got {other:?}"),
    }
    r.settings.set_selected_provider(ProviderId::Claude);
    assert_eq!(r.host.status().refresh, RefreshStatus::None);
}

/// The host opens a sign-in window and signs out through the service.
#[tokio::test]
async fn test_host_signs_in_and_out_through_the_service() {
    let r = rig();
    r.host.sign_in(ProviderId::Grok);
    assert_eq!(*r.web.opened.lock().expect("lock"), vec!["grok"]);
    r.host.sign_out(ProviderId::Grok);
    assert!(
        r.web
            .signed_out
            .lock()
            .expect("lock")
            .contains(&"grok".to_owned())
    );
}

/// With the Overview selected and nothing refreshed yet there are no lines.
#[tokio::test]
async fn test_refresh_status_on_the_overview_is_empty_before_any_refresh() {
    let r = rig_with(false);
    r.settings.set_selected_provider(ProviderId::Overview);
    assert_eq!(r.host.status().refresh, RefreshStatus::None);
}

/// Providers can be switched back on for alerts and for polling after being
/// switched off.
#[tokio::test]
async fn test_backend_re_enables_alerts_and_providers() {
    let r = rig();
    let backend = AppSettingsBackend::new(Arc::clone(&r.settings));
    let alerts = |b: &AppSettingsBackend| b.snapshot().alert_providers;
    let enabled = |b: &AppSettingsBackend| b.snapshot().enabled_providers;
    backend.apply(SettingsChange::AlertProvider(ProviderId::Claude, false));
    assert!(!alerts(&backend).contains(&ProviderId::Claude));
    backend.apply(SettingsChange::AlertProvider(ProviderId::Claude, true));
    assert!(alerts(&backend).contains(&ProviderId::Claude));
    backend.apply(SettingsChange::ProviderEnabled(ProviderId::Cursor, false));
    assert!(!enabled(&backend).contains(&ProviderId::Cursor));
    backend.apply(SettingsChange::ProviderEnabled(ProviderId::Cursor, true));
    assert!(enabled(&backend).contains(&ProviderId::Cursor));
}
