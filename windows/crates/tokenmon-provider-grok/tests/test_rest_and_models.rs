//! Snapshot model, product catalog and auth-policy tests.
#![allow(
    clippy::unwrap_used,
    clippy::assert_is_empty,
    clippy::needless_pass_by_value
)]

mod common;

use std::collections::HashSet;

use common::utc;
use tokenmon_core::net::Domain;
use tokenmon_provider_grok::auth::{
    CookieRecord, auth_config, is_auth_host, is_grok_domain, is_preferred_session_cookie,
    is_return_page, select_session_cookies,
};
use tokenmon_provider_grok::models::{
    ProductCatalog, ProductColor, ProductUsage, WeeklyUsageSnapshot, clamp_percent, fill_fraction,
    synthesize_products,
};

fn visible(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|s| (*s).to_owned()).collect()
}

fn p(id: &str, pct: f64) -> ProductUsage {
    ProductUsage::new(id, ProductCatalog::display_name(id), pct)
}

fn ids(products: &[ProductUsage]) -> Vec<&str> {
    products.iter().map(|p| p.id.as_str()).collect()
}

/// `WeeklyUsageSnapshot::new` derives remaining; values clamp at the boundary.
#[test]
fn test_remaining_defaults_and_clamping() {
    let now = utc(2026, 1, 1, 0, 0);
    assert_eq!(WeeklyUsageSnapshot::new(now, 40.0).remaining_percent, 60.0);
    let s = WeeklyUsageSnapshot::new(now, 140.0).with_remaining(-20.0);
    assert_eq!((s.used_percent, s.remaining_percent), (100.0, 0.0));
    assert_eq!(
        ProductUsage::new("chat", "Chat", 140.0).percent_of_pool,
        100.0
    );
    assert_eq!(clamp_percent(37.6), 37.6);
}

/// Color mapping is case-insensitive.
#[test]
fn test_product_color_mapping() {
    assert_eq!(ProductColor::from_product_id("build"), ProductColor::Build);
    assert_eq!(ProductColor::from_product_id("API"), ProductColor::Api);
    assert_eq!(
        ProductColor::from_product_id("imagine"),
        ProductColor::Imagine
    );
    assert_eq!(
        ProductColor::from_product_id("product-9"),
        ProductColor::Other
    );
}

/// Daily cap fill fractions.
#[test]
fn test_daily_cap_fill_fraction() {
    assert!((fill_fraction(10.0) - 10.0 / (100.0 / 7.0)).abs() < 1e-12);
    assert_eq!(fill_fraction(21.0), 1.0);
    assert_eq!(fill_fraction(0.0), 0.0);
    assert_eq!(fill_fraction(0.05), 0.0);
}

/// `filtered` keeps visible products above the threshold in display order.
#[test]
fn test_filtered_keeps_visible_above_threshold() {
    let all = visible(&ProductCatalog::KNOWN_IDS);
    let products = [p("chat", 50.0), p("build", 30.0), p("api", 2.0)];
    assert_eq!(
        ids(&ProductCatalog::filtered(&products, &all, 5.0)),
        ["chat", "build"]
    );
    assert_eq!(
        ids(&ProductCatalog::filtered(
            &products,
            &visible(&["chat"]),
            0.0
        )),
        ["chat"]
    );
    let shuffled = [p("other", 5.0), p("chat", 5.0), p("build", 5.0)];
    assert_eq!(
        ids(&ProductCatalog::filtered(&shuffled, &all, 0.0)),
        ["chat", "build", "other"]
    );
    assert!(ProductCatalog::filtered(&products, &all, 99.0).is_empty());
}

/// Panel products keep the synthesized slice and drop zero values.
#[test]
fn test_panel_products() {
    let all = visible(&ProductCatalog::KNOWN_IDS);
    let synthesized = synthesize_products(64.0);
    let panel = ProductCatalog::panel_products(&synthesized, &all);
    assert_eq!(
        (panel[0].id.as_str(), panel[0].percent_of_pool),
        ("other", 64.0)
    );
    let zeros = [p("chat", 0.0), p("build", 40.0)];
    assert_eq!(
        ids(&ProductCatalog::panel_products(&zeros, &all)),
        ["build"]
    );
    assert_eq!(
        ids(&ProductCatalog::panel_products(&zeros, &visible(&["chat"]))),
        Vec::<&str>::new()
    );
    let both = [p("chat", 10.0), p("build", 40.0)];
    assert_eq!(
        ids(&ProductCatalog::panel_products(&both, &visible(&["chat"]))),
        ["chat"]
    );
}

/// The usage bar runs from the most used product to the least used.
#[test]
fn test_panel_products_orders_most_used_first() {
    let all = visible(&ProductCatalog::KNOWN_IDS);
    let products = [
        p("chat", 5.0),
        p("build", 40.0),
        p("api", 12.0),
        p("imagine", 20.0),
    ];
    assert_eq!(
        ids(&ProductCatalog::panel_products(&products, &all)),
        ["build", "imagine", "api", "chat"]
    );
}

/// Equal usage keeps the canonical display order.
#[test]
fn test_sorted_by_usage_breaks_ties_by_display_order() {
    let products = [p("voice", 10.0), p("build", 10.0), p("chat", 10.0)];
    assert_eq!(
        ids(&ProductCatalog::sorted_by_usage(&products)),
        ["chat", "build", "voice"]
    );
}

/// The category chips run from most to least used; unused ones come last in
/// display order.
#[test]
fn test_category_products_orders_most_used_first() {
    let all = visible(&ProductCatalog::KNOWN_IDS);
    let products = [p("chat", 3.0), p("voice", 9.0), p("build", 30.0)];
    assert_eq!(
        ids(&ProductCatalog::category_products(&products, &all)),
        ["build", "voice", "chat", "imagine"]
    );
}

/// Category rows always include the four categories, with 0% placeholders.
#[test]
fn test_category_products() {
    let all = visible(&ProductCatalog::KNOWN_IDS);
    let products = [p("chat", 40.0), p("api", 8.0), p("other", 7.0)];
    let rows = ProductCatalog::category_products(&products, &all);
    assert_eq!(ids(&rows), ["chat", "build", "imagine", "voice"]);
    let values: Vec<f64> = rows.iter().map(|r| r.percent_of_pool).collect();
    assert_eq!(values, [40.0, 0.0, 0.0, 0.0]);
    let some = ProductCatalog::category_products(&products, &visible(&["chat", "voice"]));
    assert_eq!(ids(&some), ["chat", "voice"]);
}

/// Persisted visible ids normalize as the spec says.
#[test]
fn test_normalize_visible_ids() {
    let stored = ["CHAT".to_owned(), "bogus".to_owned()];
    assert_eq!(
        ProductCatalog::normalize_visible_ids(&stored),
        visible(&["chat"])
    );
    assert_eq!(
        ProductCatalog::normalize_visible_ids(&["bogus".to_owned()]).len(),
        6
    );
    assert!(ProductCatalog::normalize_visible_ids(&[]).is_empty());
}

/// Display names capitalize unknown ids like Foundation.
#[test]
fn test_display_names() {
    assert_eq!(ProductCatalog::display_name("grok build"), "Grok Build");
    assert_eq!(
        ProductCatalog::display_name("some new thing"),
        "Some New Thing"
    );
    assert_eq!(ProductCatalog::display_name("product-9"), "Product-9");
    assert_eq!(ProductCatalog::short_name("build"), "Build");
    assert_eq!(ProductCatalog::short_name("API"), "API");
}

/// Lookalike domains are rejected.
#[test]
fn test_grok_domain_matching_does_not_accept_lookalikes() {
    assert!(is_grok_domain(".grok.com"));
    assert!(is_grok_domain("accounts.x.ai"));
    assert!(!is_grok_domain("evilgrok.com"));
    assert!(!is_grok_domain("x.ai.evil.example"));
    assert!(Domain::matches("WWW.Grok.COM", &["grok.com"]));
    assert!(!Domain::matches("grok.com.evil.example", &["grok.com"]));
}

/// X and Twitter are identity hosts, not Grok session domains.
#[test]
fn test_grok_domain_excludes_x_and_twitter() {
    assert!(!is_grok_domain("x.com"));
    assert!(!is_grok_domain(".twitter.com"));
    assert!(is_auth_host("x.com"));
    assert!(is_auth_host("accounts.x.ai"));
    assert!(is_auth_host("api.x.com"));
    assert!(!is_auth_host("netflix.com"));
    assert!(!is_auth_host("x.com.evil.example"));
}

/// The return page is any grok.com URL; lookalikes are rejected.
#[test]
fn test_grok_return_page_rejects_lookalike_host() {
    assert!(is_return_page("https://grok.com/?_s=usage"));
    assert!(is_return_page("https://www.grok.com/"));
    assert!(!is_return_page("https://grok.com.evil.example/"));
    assert!(!is_return_page("not a url"));
}

fn cookie(name: &str, domain: &str) -> CookieRecord {
    CookieRecord {
        name: name.to_owned(),
        value: format!("{name}-v"),
        domain: domain.to_owned(),
    }
}

/// Only the grok.com session cookie is persisted; X session cookies and
/// trackers in the same jar are dropped.
#[test]
fn test_grok_policy_drops_x_session_cookies() {
    let jar = [
        cookie("sso", "grok.com"),
        cookie("auth_token", "x.com"),
        cookie("ct0", "x.com"),
        cookie("_ga", "grok.com"),
    ];
    assert_eq!(select_session_cookies(&jar).as_deref(), Some("sso=sso-v"));
}

/// A jar without `sso` / `sso-rw` captures nothing.
#[test]
fn test_grok_policy_without_session_cookie_captures_nothing() {
    let jar = [
        cookie("x-session-hint", "grok.com"),
        cookie("_ga", "grok.com"),
    ];
    assert!(select_session_cookies(&jar).is_none());
    let rw_only = [cookie("SSO-RW", "grok.com")];
    assert!(select_session_cookies(&rw_only).is_some());
}

/// The `AuthConfig` carries the spec constants.
#[test]
fn test_auth_config_constants() {
    let cfg = auth_config();
    assert_eq!(cfg.hosts, ["grok.com", "x.ai"]);
    assert_eq!(cfg.sign_out_hosts, ["grok.com", "x.ai"]);
    assert_eq!(cfg.max_attempts, 4);
    assert!(cfg.is_essential("sso") && cfg.is_essential("sso-rw") && !cfg.is_essential("_ga"));
    assert!(is_preferred_session_cookie("SSO") && is_preferred_session_cookie("sso-rw"));
}

/// Regression (#53): a jar with only `sso-rw` yields a stored cookie header.
#[test]
fn test_select_session_cookies_accepts_sso_rw_only() {
    let jar = [CookieRecord {
        name: "sso-rw".into(),
        value: "rw".into(),
        domain: ".grok.com".into(),
    }];
    assert_eq!(select_session_cookies(&jar).as_deref(), Some("sso-rw=rw"));
    assert!(auth_config().is_preferred_session_cookie("SSO-RW"));
}

/// `testGrokIsWeekly`.
#[test]
fn test_grok_is_weekly() {
    let s = WeeklyUsageSnapshot::new(chrono::DateTime::UNIX_EPOCH, 10.0);
    assert_eq!(s.usage_pool(), tokenmon_core::usage::UsagePool::Weekly);
}
