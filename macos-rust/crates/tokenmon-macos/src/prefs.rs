//! One-time, read-only import of the Swift app's non-secret preferences
//! (enabled providers, selected tab, menu-bar graphs) so the beta opens with the
//! same tabs. The input is the Swift app's `UserDefaults` domain exported as
//! JSON; nothing is written back to it.

use std::collections::BTreeSet;

use serde::Deserialize;
use tokenmon_core::ProviderId;
use tokenmon_core::settings::AppSettings;

/// The preferences the beta understands. Unknown or invalid values are skipped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SwiftPrefs {
    /// `enabledProviderIDs`.
    pub enabled: BTreeSet<ProviderId>,
    /// `menuBarGraphProviders`.
    pub menu_bar_graphs: BTreeSet<ProviderId>,
    /// `selectedProvider`.
    pub selected: Option<ProviderId>,
    /// `showSelectedProviderInMenuBar`.
    pub show_selected_in_menu_bar: Option<bool>,
    /// `showCategoriesInMenuBar`.
    pub show_categories_in_menu_bar: Option<bool>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Wire {
    #[serde(rename = "enabledProviderIDs")]
    enabled: Vec<String>,
    #[serde(rename = "menuBarGraphProviders")]
    graphs: Vec<String>,
    #[serde(rename = "selectedProvider")]
    selected: Option<String>,
    #[serde(rename = "showSelectedProviderInMenuBar")]
    show_selected: Option<bool>,
    #[serde(rename = "showCategoriesInMenuBar")]
    show_categories: Option<bool>,
}

fn providers(raw: &[String]) -> BTreeSet<ProviderId> {
    raw.iter().filter_map(|s| s.parse().ok()).collect()
}

impl SwiftPrefs {
    /// Parses the JSON export; `None` when it is not a JSON object.
    ///
    /// ```
    /// use tokenmon_macos::prefs::SwiftPrefs;
    /// let p = SwiftPrefs::from_json(r#"{"enabledProviderIDs":["claude","grok"],"selectedProvider":"claude"}"#).unwrap();
    /// assert_eq!(p.enabled.len(), 2);
    /// ```
    pub fn from_json(json: &str) -> Option<Self> {
        let wire: Wire = serde_json::from_str(json).ok()?;
        Some(Self {
            enabled: providers(&wire.enabled),
            menu_bar_graphs: providers(&wire.graphs),
            selected: wire.selected.and_then(|s| s.parse().ok()),
            show_selected_in_menu_bar: wire.show_selected,
            show_categories_in_menu_bar: wire.show_categories,
        })
    }

    /// Applies the preferences to the beta's own settings (its JSON file).
    pub fn apply(&self, settings: &AppSettings) {
        if !self.enabled.is_empty() {
            let _ = settings.set_enabled_providers(&self.enabled);
        }
        for p in ProviderId::USAGE_PROVIDERS {
            if p.supports_menu_bar_graph() || p == ProviderId::Grok {
                settings.set_menu_bar_graph(p, self.menu_bar_graphs.contains(&p));
            }
        }
        if let Some(p) = self.selected {
            settings.set_selected_provider(p);
        }
        if let Some(on) = self.show_selected_in_menu_bar {
            settings.set_show_selected_provider_in_menu_bar(on);
        }
        if let Some(on) = self.show_categories_in_menu_bar {
            settings.set_show_categories_in_menu_bar(on);
        }
    }
}
