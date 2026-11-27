//! The Swift preference import and the beta's stand-in platform services.

use std::sync::{Arc, Mutex};

use tokenmon_core::ProviderId;
use tokenmon_core::settings::{AppSettings, SettingsFile};
use tokenmon_core::update::UrlOpener;
use tokenmon_macos::prefs::SwiftPrefs;
use tokenmon_macos::services::{BetaDesktopVerb, BetaLoginItem, HttpsOpener};
use tokenmon_platform::shell::{LoginItemService, LoginItemStatus, ShellVerbService};

const EXPORT: &str = r#"{"enabledProviderIDs":["chatgpt","claude","grok","bogus"],
  "menuBarGraphProviders":["grok"],"selectedProvider":"claude",
  "showSelectedProviderInMenuBar":true,"showCategoriesInMenuBar":false,"other":1}"#;

/// The JSON export is parsed; unknown providers are skipped.
#[test]
fn test_prefs_parse_known_keys_and_skip_unknown_providers() {
    let p = SwiftPrefs::from_json(EXPORT).expect("parsed");
    let want = |ids: &[ProviderId]| ids.iter().copied().collect();
    assert_eq!(
        p.enabled,
        want(&[ProviderId::ChatGpt, ProviderId::Claude, ProviderId::Grok])
    );
    assert_eq!(p.menu_bar_graphs, want(&[ProviderId::Grok]));
    assert_eq!(p.selected, Some(ProviderId::Claude));
    assert_eq!(p.show_selected_in_menu_bar, Some(true));
    assert_eq!(p.show_categories_in_menu_bar, Some(false));
}

/// Garbage or a non-object is not preferences.
#[test]
fn test_prefs_reject_garbage_and_accept_empty_object() {
    assert!(SwiftPrefs::from_json("not json").is_none());
    assert!(SwiftPrefs::from_json("[1]").is_none());
    assert_eq!(
        SwiftPrefs::from_json("{}").expect("empty"),
        SwiftPrefs::default()
    );
}

/// Applying the import sets the beta's own settings (written to its JSON file).
#[test]
fn test_prefs_apply_to_the_betas_own_settings_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = Arc::new(SettingsFile::open(dir.path().join("settings.json")));
    let settings = AppSettings::new(file);
    SwiftPrefs::from_json(EXPORT)
        .expect("parsed")
        .apply(&settings);
    let s = settings.snapshot();
    assert_eq!(s.selected_provider, ProviderId::Claude);
    assert!(s.is_provider_enabled(ProviderId::Grok));
    assert!(!s.is_provider_enabled(ProviderId::Cursor));
    assert!(s.show_selected_provider_in_menu_bar);
    assert!(s.menu_bar_graph_providers.contains(&ProviderId::Grok));
}

/// Preferences with no enabled providers leave the defaults alone.
#[test]
fn test_prefs_without_enabled_providers_keep_defaults() {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = AppSettings::new(Arc::new(SettingsFile::open(dir.path().join("s.json"))));
    let before = settings.snapshot().enabled_provider_ids;
    SwiftPrefs::from_json("{}").expect("empty").apply(&settings);
    assert_eq!(settings.snapshot().enabled_provider_ids, before);
}

/// The stand-in services report "not available" honestly.
#[test]
fn test_stand_in_services_report_unavailable() {
    assert_eq!(
        BetaLoginItem.status().expect("status"),
        LoginItemStatus::NotRegistered
    );
    assert!(BetaLoginItem.register().is_err());
    assert!(BetaLoginItem.unregister().is_ok());
    assert!(!BetaDesktopVerb.is_installed().expect("state"));
    assert!(BetaDesktopVerb.install().is_err());
    assert!(BetaDesktopVerb.uninstall().is_ok());
}

/// Only https URLs reach the launcher.
#[test]
fn test_opener_passes_only_https() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&seen);
    let opener = HttpsOpener::new(Arc::new(move |u| {
        sink.lock().expect("lock").push(u.to_owned());
    }));
    opener.open("https://github.com/x");
    opener.open("file:///etc/passwd");
    opener.open("http://insecure");
    assert_eq!(*seen.lock().expect("lock"), ["https://github.com/x"]);
}
