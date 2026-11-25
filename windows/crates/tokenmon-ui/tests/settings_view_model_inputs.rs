//! Rejection and edge paths of the Settings view model and its in-memory
//! backend: unknown provider ids, unsupported rows, no-op drags and the
//! backend's own invariants.
#![allow(missing_docs)]

mod common;

use common::fixture;
use tokenmon_core::ProviderId;
use tokenmon_ui::settings::fake::{HostCall, InMemorySettings};
use tokenmon_ui::settings::{SettingsBackend, SettingsChange, SettingsSnapshot};

/// An unknown provider id never reaches the backend for any per-provider
/// intent, so settings stay exactly as they were.
#[test]
fn test_unknown_provider_ids_change_nothing() {
    let f = fixture(None);
    let before = f.settings.snapshot();
    f.vm.set_alert_provider("nope", true);
    f.vm.set_provider_enabled("nope", false);
    f.vm.set_menu_bar_graph("nope", true);
    f.vm.sign_in("nope");
    f.vm.sign_out("nope");
    assert_eq!(f.settings.snapshot(), before);
    assert_eq!(f.host.calls(), []);
}

/// Providers without a menu-bar graph (Overview, OpenRouter, ChatGPT) ignore
/// the toggle.
#[test]
fn test_menu_bar_graph_ignores_providers_without_one() {
    let f = fixture(None);
    let before = f.settings.snapshot();
    for id in ["overview", "openrouter", "chatgpt"] {
        f.vm.set_menu_bar_graph(id, true);
    }
    assert_eq!(f.settings.snapshot(), before);
}

/// Dropping a provider on its own row is a no-op.
#[test]
fn test_move_provider_onto_itself_is_a_noop() {
    let f = fixture(None);
    let before = f.settings.snapshot().provider_order;
    f.vm.move_provider("grok", 0);
    assert_eq!(f.settings.snapshot().provider_order, before);
}

/// The per-provider alert switch round-trips on and off.
#[test]
fn test_alert_provider_switch_round_trips() {
    let f = fixture(None);
    f.vm.set_alert_provider("claude", false);
    assert!(
        !f.settings
            .snapshot()
            .alert_providers
            .contains(&ProviderId::Claude)
    );
    f.vm.set_alert_provider("claude", true);
    assert!(
        f.settings
            .snapshot()
            .alert_providers
            .contains(&ProviderId::Claude)
    );
}

/// A failed desktop-menu write is reported and leaves the real state off.
#[test]
fn test_failed_desktop_menu_reverts_and_reports() {
    let f = fixture(None);
    *f.host.fail_with.lock().unwrap() = Some("shell denied".into());
    f.vm.set_desktop_menu(true);
    let g = f.vm.screen().general;
    assert!(!g.desktop_menu);
    assert_eq!(g.refresh_error, "shell denied");
    assert_eq!(f.host.calls(), [HostCall::DesktopMenu(true)]);
}

/// Disabling the selected provider moves the selection to Overview.
#[test]
fn test_disabling_selected_provider_selects_overview() {
    let backend = InMemorySettings::new(SettingsSnapshot {
        selected_provider: ProviderId::Cursor,
        ..SettingsSnapshot::default()
    });
    backend.apply(SettingsChange::ProviderEnabled(ProviderId::Cursor, false));
    let snap = backend.snapshot();
    assert!(!snap.enabled_providers.contains(&ProviderId::Cursor));
    assert_eq!(snap.selected_provider, ProviderId::Overview);
}

/// The backend itself refuses to empty the enabled set.
#[test]
fn test_backend_never_empties_enabled_providers() {
    let f = fixture(None);
    let all = f.settings.snapshot().enabled_providers;
    for p in &all {
        f.settings.apply(SettingsChange::ProviderEnabled(*p, false));
    }
    assert_eq!(f.settings.snapshot().enabled_providers.len(), 1);
}

/// Out-of-range theme indexes are ignored.
#[test]
fn test_theme_index_out_of_range_is_ignored() {
    let f = fixture(None);
    let before = f.settings.snapshot().theme_preference;
    f.vm.set_theme_index(99);
    f.vm.set_theme_index(-1);
    assert_eq!(f.settings.snapshot().theme_preference, before);
}
