//! Ports of `ClaudeUsageClientTests.swift` plus poller helper tests
//! (spec 03 sections 7.2 and 7.4).
#![allow(clippy::unwrap_used, clippy::assert_is_empty)]

use chrono::{Datelike, TimeZone, Timelike, Utc};
use tokenmon_core::{ProviderError, UsageError};
use tokenmon_provider_claude::usage::usage_request;
use tokenmon_provider_claude::{
    ClaudeSnapshot, ClaudeUsageWindow, organization_id_from_cookie_header, parse_usage_body,
};

const FIXTURE: &str = r#"{"five_hour":{"utilization":42.5,"resets_at":"2026-08-21T14:00:00.127279+00:00"},
"seven_day":{"utilization":87.0,"resets_at":"2026-08-27T11:00:00+00:00"}}"#;

fn parse(s: &str) -> tokenmon_provider_claude::ClaudeUsageResponse {
    parse_usage_body(s.as_bytes()).unwrap()
}

/// Both windows parse with their percentages.
#[test]
fn test_parse_windows() {
    let r = parse(FIXTURE);
    assert_eq!(r.five_hour.unwrap().used_percent, 42.5);
    assert_eq!(r.seven_day.unwrap().used_percent, 87.0);
}

/// Reset dates parse with and without fractional seconds.
#[test]
fn test_parse_reset_dates() {
    let r = parse(FIXTURE);
    assert_eq!(r.five_hour.unwrap().resets_at.unwrap().hour(), 14);
    assert_eq!(r.seven_day.unwrap().resets_at.unwrap().hour(), 11);
}

/// An empty object has neither window.
#[test]
fn test_parse_missing_windows_yields_none() {
    let r = parse("{}");
    assert!(r.five_hour.is_none() && r.seven_day.is_none());
}

/// Utilization above 100 clamps.
#[test]
fn test_parse_clamps_out_of_range_utilization() {
    let r = parse(r#"{"five_hour":{"utilization":130}}"#);
    assert_eq!(r.five_hour.unwrap().used_percent, 100.0);
    let r = parse(r#"{"five_hour":{"utilization":-4}}"#);
    assert_eq!(r.five_hour.unwrap().used_percent, 0.0);
}

/// A login page is an auth failure.
#[test]
fn test_parse_html_body_is_unauthorized() {
    let e = parse_usage_body(b"<!doctype html><html><body>Sign in to Claude</body></html>")
        .unwrap_err();
    assert!(matches!(e, ProviderError::Unauthorized(_)));
}

/// A Cloudflare challenge page is a bad response, not an auth failure.
#[test]
fn test_parse_challenge_html_is_bad_response() {
    let e = parse_usage_body(b"<html>Just a moment... sign in cf-chl</html>").unwrap_err();
    assert!(matches!(&e, ProviderError::BadResponse(_, m) if m == "Unexpected HTML response"));
}

/// Non-JSON, non-HTML bodies are malformed.
#[test]
fn test_parse_malformed_body_is_bad_response() {
    assert!(matches!(
        parse_usage_body(b"not json").unwrap_err(),
        ProviderError::BadResponse(_, m) if m == "Malformed response body"
    ));
    assert!(parse_usage_body(b"[1,2]").is_err());
}

/// Org id comes from the case-sensitive `lastActiveOrg` pair.
#[test]
fn test_organization_id_from_cookie_header() {
    let id = "9d7a1111-2222-3333-4444-555555555678";
    let h = format!("__cf_bm=abc; lastActiveOrg={id}; sessionKey=zzz");
    assert_eq!(organization_id_from_cookie_header(&h).as_deref(), Some(id));
    assert_eq!(organization_id_from_cookie_header("sessionKey=abc"), None);
    assert_eq!(organization_id_from_cookie_header(""), None);
    assert_eq!(organization_id_from_cookie_header("lastactiveorg=x"), None);
    assert_eq!(
        organization_id_from_cookie_header("lastActiveOrg= ; lastActiveOrg=y"),
        None
    );
}

/// No org id is a not-signed-in strike; with one the URL is built verbatim.
#[test]
fn test_usage_request_needs_org() {
    let e = usage_request("sessionKey=a").unwrap_err();
    assert!(matches!(
        e,
        ProviderError::Custom {
            usage: UsageError::NotSignedIn,
            ..
        }
    ));
    let r = usage_request("lastActiveOrg=abc; sessionKey=a").unwrap();
    assert_eq!(
        r.url,
        "https://claude.ai/api/organizations/abc/usage?cedar_ember=1"
    );
    assert_eq!(r.referer, "https://claude.ai/");
    assert_eq!(r.path, "/api/organizations/abc/usage?cedar_ember=1");
}

const LIMITS: &str = r#"{"five_hour":null,"seven_day":null,"limits":[
 {"kind":"session","group":"session","percent":14,"resets_at":"2026-10-04T13:00:00Z","scope":null,"is_active":true},
 {"kind":"weekly_scoped","group":"weekly","percent":3,"resets_at":"2026-10-11T08:00:00Z",
  "scope":{"model":{"id":"claude-fable-5","display_name":"Fable"},"surface":"x"},"is_active":false}]}"#;

/// Without `seven_day`, the weekly limit entry fills in, including its scope.
#[test]
fn test_parse_falls_back_to_weekly_limit_when_seven_day_is_null() {
    let r = parse(LIMITS);
    let w = r.seven_day.clone().unwrap();
    assert_eq!(w.used_percent, 3.0);
    assert_eq!(w.scope_name.as_deref(), Some("Fable"));
    assert_eq!(
        w.resets_at,
        Some(Utc.with_ymd_and_hms(2026, 10, 11, 8, 0, 0).unwrap())
    );
    assert_eq!(r.five_hour.clone().unwrap().used_percent, 14.0);
    let snap = ClaudeSnapshot {
        fetched_at: Utc::now(),
        five_hour: r.five_hour,
        seven_day: r.seven_day,
        account_email: None,
        saved_resets: None,
    };
    assert_eq!(snap.weekly_label(), "Weekly \u{b7} Fable");
}

/// An account-wide limit beats a scoped active one.
#[test]
fn test_parse_prefers_account_wide_weekly_limit() {
    let body = r#"{"limits":[
 {"group":"weekly","percent":80,"resets_at":"2026-10-11T08:00:00Z","scope":{"model":{"id":"m"}},"is_active":true},
 {"group":"weekly","percent":20,"scope":null,"is_active":false}]}"#;
    let w = parse(body).seven_day.unwrap();
    assert_eq!(w.used_percent, 20.0);
    assert!(w.scope_name.is_none() && w.resets_at.is_none());
}

/// A top-level `seven_day` object wins over `limits`.
#[test]
fn test_parse_seven_day_object_wins_over_limits() {
    let body = r#"{"seven_day":{"utilization":40},"limits":[{"group":"weekly","percent":5}]}"#;
    assert_eq!(parse(body).seven_day.unwrap().used_percent, 40.0);
}

/// Ties keep the first entry; numeric strings and `{val}` objects count as numbers.
#[test]
fn test_limit_ranking_ties_and_flexible_numbers() {
    let body = r#"{"limits":[
 {"group":"weekly","percent":"7.5","resets_at":"2026-10-11T08:00:00Z"},
 {"group":"weekly","percent":{"val":7.5},"resets_at":"2026-10-12T08:00:00Z"}]}"#;
    let w = parse(body).seven_day.unwrap();
    assert_eq!(w.used_percent, 7.5);
    assert_eq!(w.resets_at.unwrap().day(), 11);
}

/// NaN/Infinity-like strings never become percents.
#[test]
fn test_non_finite_numbers_are_rejected() {
    let r = parse(r#"{"five_hour":{"utilization":"nan"},"seven_day":{"utilization":"inf"}}"#);
    assert!(r.five_hour.is_none() && r.seven_day.is_none());
}

/// Snapshot headline, reset and pool rules.
#[test]
fn test_snapshot_headline_resets_and_pool() {
    let w = |p, reset| ClaudeUsageWindow {
        used_percent: p,
        resets_at: reset,
        scope_name: None,
    };
    let reset = Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap());
    let mut s = ClaudeSnapshot {
        fetched_at: Utc::now(),
        five_hour: None,
        seven_day: Some(w(30.0, reset)),
        account_email: None,
        saved_resets: None,
    };
    assert_eq!(
        (s.headline_used_percent(), s.resets_at(), s.weekly_label()),
        (30.0, reset, "Weekly".to_owned())
    );
    assert_eq!(s.usage_pool().section_title(), "Usage Pool Weekly");
    s.five_hour = Some(w(10.0, None));
    assert_eq!((s.headline_used_percent(), s.resets_at()), (10.0, reset));
    assert_eq!(s.usage_pool().section_title(), "Usage Pool Mixed");
    assert_eq!(s.seven_day.as_ref().unwrap().remaining_percent(), 70.0);
}

/// `testClaudePools`: both windows are Mixed, 5-hour alone Hourly, weekly alone Weekly.
#[test]
fn test_claude_pools() {
    use tokenmon_core::usage::UsagePool;
    let w = |p| ClaudeUsageWindow {
        used_percent: p,
        resets_at: None,
        scope_name: None,
    };
    let snap = |five_hour, seven_day| ClaudeSnapshot {
        fetched_at: Utc::now(),
        five_hour,
        seven_day,
        account_email: None,
        saved_resets: None,
    };
    assert_eq!(
        snap(Some(w(10.0)), Some(w(20.0))).usage_pool(),
        UsagePool::Mixed
    );
    assert_eq!(snap(Some(w(10.0)), None).usage_pool(), UsagePool::Hourly);
    assert_eq!(snap(None, Some(w(20.0))).usage_pool(), UsagePool::Weekly);
}

// -- Saved resets (cedar_ember) ----------------------------------------------

fn reset_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 22, 12, 0, 0).unwrap()
}

fn saved_resets(block: &str) -> Option<tokenmon_core::usage::SavedResets> {
    let body = format!(r#"{{"cedar_ember": {block}}}"#);
    tokenmon_provider_claude::parse_usage_body_at(body.as_bytes(), reset_now())
        .unwrap()
        .saved_resets
}

/// `testSavedResetsSumAvailableGrants`: counts add up across grants and the
/// soonest `ends_at` is the next expiry.
#[test]
fn test_saved_resets_sum_available_grants() {
    let resets = saved_resets(
        r#"{"eligible": true, "grants": [
          {"id": "g1", "resets_left": 1, "resets_total": 1, "paused": false,
           "starts_at": "2026-11-01T00:00:00Z", "ends_at": "2026-12-20T00:00:00Z"},
          {"id": "g2", "resets_left": 2, "resets_total": 2, "paused": false,
           "starts_at": null, "ends_at": "2026-12-05T00:00:00Z"},
          {"id": "g3", "resets_left": 1, "paused": false}
        ]}"#,
    )
    .unwrap();
    assert_eq!(resets.count(), 4);
    assert_eq!(
        resets.next_expiry(),
        Some(Utc.with_ymd_and_hms(2026, 12, 5, 0, 0, 0).unwrap())
    );
}

/// `testSavedResetsSkipUnavailableGrants`: paused, used-up, not-started and
/// expired grants offer nothing.
#[test]
fn test_saved_resets_skip_unavailable_grants() {
    assert!(
        saved_resets(
            r#"{"eligible": true, "grants": [
              {"resets_left": 1, "paused": true},
              {"resets_left": 0, "resets_total": 1, "paused": false},
              {"resets_left": 1, "paused": false, "starts_at": "2026-12-01T00:00:00Z"},
              {"resets_left": 1, "paused": false, "ends_at": "2026-11-22T12:00:00Z"}
            ]}"#
        )
        .is_none()
    );
}

/// `testSavedResetsDropMalformedGrants`: a malformed grant is dropped; the
/// well-formed one still counts.
#[test]
fn test_saved_resets_drop_malformed_grants() {
    let resets = saved_resets(
        r#"{"eligible": true, "grants": [
          {"resets_left": "many", "paused": false},
          {"resets_left": 2, "resets_total": 1, "paused": false},
          {"resets_left": -1, "paused": false},
          {"resets_left": 1},
          {"resets_left": 1, "paused": null},
          {"resets_left": 1, "paused": false, "ends_at": "next tuesday"},
          {"resets_left": 1, "paused": false, "starts_at": 5},
          {"resets_left": 1, "resets_total": 1, "paused": false}
        ]}"#,
    )
    .unwrap();
    assert_eq!(resets.count(), 1);
    assert!(resets.next_expiry().is_none());
}

/// `testSavedResetsNeedEligibleBlock`: ineligible, missing, null or
/// grant-less blocks and the plain fixture show nothing.
#[test]
fn test_saved_resets_need_eligible_block() {
    let grant = r#"{"resets_left": 1, "paused": false}"#;
    assert!(saved_resets(&format!(r#"{{"eligible": false, "grants": [{grant}]}}"#)).is_none());
    assert!(saved_resets(&format!(r#"{{"grants": [{grant}]}}"#)).is_none());
    assert!(saved_resets(r#"{"eligible": true}"#).is_none());
    assert!(saved_resets("null").is_none());
    assert!(parse(FIXTURE).saved_resets.is_none());
}

/// `testSavedResetsImplausibleInventoryShowsNothing`: a payload claiming
/// 5000 resets is treated as malformed.
#[test]
fn test_saved_resets_implausible_inventory_shows_nothing() {
    assert!(
        saved_resets(r#"{"eligible": true, "grants": [{"resets_left": 5000, "paused": false}]}"#)
            .is_none()
    );
}
