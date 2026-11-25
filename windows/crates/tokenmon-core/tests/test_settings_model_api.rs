//! The `Settings` value API and the settings enums: string forms, theme
//! resolution, poll cadence and the builder-style setters.

use std::collections::BTreeSet;

use tokenmon_core::AppTheme;
use tokenmon_core::ProviderId::{self, *};
use tokenmon_core::settings::{GrokProduct, PanelSection, Settings, ThemePreference};

/// Every settings enum survives `Display` then `FromStr`, and rejects an
/// unknown spelling with a message naming the kind and the value.
#[test]
fn test_settings_enums_roundtrip_through_strings_and_reject_unknowns() {
    for p in GrokProduct::ALL {
        assert_eq!(p.to_string().parse::<GrokProduct>().expect("roundtrip"), *p);
    }
    for s in PanelSection::ALL {
        assert_eq!(
            s.to_string().parse::<PanelSection>().expect("roundtrip"),
            *s
        );
    }
    for t in ThemePreference::ALL {
        assert_eq!(t.to_string(), t.as_str());
        assert_eq!(
            t.to_string().parse::<ThemePreference>().expect("roundtrip"),
            *t
        );
    }
    let e = "neon"
        .parse::<ThemePreference>()
        .expect_err("unknown theme");
    assert_eq!(e.to_string(), "unknown theme preference value: \"neon\"");
    assert!("video".parse::<GrokProduct>().is_err());
    assert!("".parse::<PanelSection>().is_err());
}

/// `System` follows the OS; explicit choices ignore it.
#[test]
fn test_theme_preference_resolves_against_the_system_theme() {
    for system in [AppTheme::Light, AppTheme::Dark] {
        assert_eq!(ThemePreference::System.resolve(system), system);
        assert_eq!(ThemePreference::Light.resolve(system), AppTheme::Light);
        assert_eq!(ThemePreference::Dark.resolve(system), AppTheme::Dark);
    }
}

/// The poll cadence depends on whether the menu is open.
#[test]
fn test_poll_seconds_follow_menu_state() {
    let s = Settings::default();
    assert_eq!(s.poll_seconds(true), s.active_poll_seconds);
    assert_eq!(s.poll_seconds(false), s.idle_poll_seconds);
    assert_ne!(s.active_poll_seconds, s.idle_poll_seconds);
}

/// The menu-bar graph set only accepts providers that have a graph, and can
/// be turned off again.
#[test]
fn test_menu_bar_graph_setter_and_query() {
    let s = Settings::default();
    assert!(s.shows_menu_bar_graph(Grok));
    assert!(!s.shows_menu_bar_graph(Claude));
    let on = s.clone().with_menu_bar_graph(Claude, true);
    assert!(on.shows_menu_bar_graph(Claude));
    let off = on.with_menu_bar_graph(Claude, false);
    assert!(!off.shows_menu_bar_graph(Claude));
    let unsupported = ProviderId::Overview;
    let same = s.clone().with_menu_bar_graph(unsupported, true);
    assert_eq!(same.menu_bar_graph_providers, s.menu_bar_graph_providers);
}

/// Sections collapse and expand, and expanding twice is idempotent.
#[test]
fn test_expanded_sections_toggle() {
    let section = PanelSection::ALL[0];
    let s = Settings::default();
    assert!(s.is_expanded(section));
    let collapsed = s.with_expanded(section, false);
    assert!(!collapsed.is_expanded(section));
    let reopened = collapsed
        .with_expanded(section, true)
        .with_expanded(section, true);
    assert!(reopened.is_expanded(section));
}

/// Visible products can be replaced, including by an empty set.
#[test]
fn test_visible_products_are_replaced_wholesale() {
    let s = Settings::default();
    let some = s
        .clone()
        .with_visible_products(BTreeSet::from([GrokProduct::Chat]));
    assert_eq!(
        some.visible_product_ids,
        BTreeSet::from([GrokProduct::Chat])
    );
    assert!(
        s.with_visible_products(BTreeSet::new())
            .visible_product_ids
            .is_empty()
    );
}

/// The alert threshold is clamped to its range and NaN becomes the default.
#[test]
fn test_threshold_is_clamped_and_nan_defaults() {
    let s = Settings::default();
    assert!(
        (s.clone().with_threshold_percent(f64::NAN).threshold_percent - 80.0).abs() < f64::EPSILON
    );
    assert!((s.clone().with_threshold_percent(10.0).threshold_percent - 50.0).abs() < f64::EPSILON);
    assert!((s.with_threshold_percent(1000.0).threshold_percent - 99.0).abs() < f64::EPSILON);
}

/// Moving a provider onto an unknown id leaves the order alone.
#[test]
fn test_move_provider_ignores_unknown_ids() {
    let s = Settings::default();
    let before = s.provider_order;
    assert_eq!(
        s.clone()
            .move_provider(ProviderId::Overview, Grok)
            .provider_order,
        before
    );
    assert_eq!(
        s.move_provider(Grok, ProviderId::Overview).provider_order,
        before
    );
}
