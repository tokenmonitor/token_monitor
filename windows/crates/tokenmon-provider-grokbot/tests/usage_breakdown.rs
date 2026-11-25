//! Ports of `GrokbotUsageBreakdownTests.swift`: parsing of
//! `GetGrokBotUsageBreakdown`, the session access token, and the client call
//! against a mock RPC host. Hermetic: wiremock only.

#![allow(clippy::unwrap_used, clippy::assert_is_empty)]

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use tokenmon_core::{FixedClock, ProviderErrorContext};
use tokenmon_provider_cursor::CursorHttp;
use tokenmon_provider_grokbot::{
    BreakdownUnit, GrokbotUsageBreakdown, GrokbotUsageClient, GrokbotUsageSource as Source,
    access_token_from_cookie_header,
};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn parse(json: &str) -> Option<GrokbotUsageBreakdown> {
    let value: Value = serde_json::from_str(json).unwrap();
    GrokbotUsageBreakdown::parse(value.as_object().unwrap())
}

fn secs(s: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(s, 0).unwrap()
}

/// Shape captured from a live response (values made up): int64 fields as
/// strings, the source as a protobuf enum name, one row per UTC day.
const WEEKLY_FIXTURE: &str = r#"
{"weeklyLimitPpm":{"periodStartMs":"1794425100000","periodEndMs":"1795029900000",
 "bySource":[
  {"dayMs":"1794355200000","source":"GROK_BOT_USAGE_SOURCE_USER","ppm":100000},
  {"dayMs":"1794441600000","source":"GROK_BOT_USAGE_SOURCE_USER","ppm":150000},
  {"dayMs":"1794441600000","source":"GROK_BOT_USAGE_SOURCE_AUTOMATION","ppm":300000},
  {"dayMs":"1794441600000","source":"GROK_BOT_USAGE_SOURCE_AGENT","ppm":120000}],
 "byBot":[{"dayMs":"1794441600000","botId":"bot_1","ppm":670000}]}}
"#;

/// Days sum per source as percent of the weekly allowance, most used first.
#[test]
fn test_weekly_breakdown_sums_days_per_source_as_percent() {
    let b = parse(WEEKLY_FIXTURE).unwrap();
    assert_eq!(b.unit, BreakdownUnit::WeeklyLimitPercent);
    let sources: Vec<Source> = b.slices.iter().map(|s| s.source).collect();
    assert_eq!(sources, [Source::Automation, Source::User, Source::Agent]);
    let values: Vec<f64> = b.slices.iter().map(|s| s.value).collect();
    assert_eq!(values, [30.0, 25.0, 12.0]);
    assert!((b.total() - 67.0).abs() < 1e-9);
    assert_eq!(b.period_start, Some(secs(1_794_425_100)));
    assert_eq!(b.period_end, Some(secs(1_795_029_900)));
}

/// Billed cents become dollars; sources may be names or numbers.
#[test]
fn test_billed_cents_become_dollars() {
    let b = parse(
        r#"{"billed_cents":{"by_source":[{"source":"USER","cents":"250"},{"source":2,"cents":"125"}]}}"#,
    )
    .unwrap();
    assert_eq!(b.unit, BreakdownUnit::Usd);
    let sources: Vec<Source> = b.slices.iter().map(|s| s.source).collect();
    assert_eq!(sources, [Source::User, Source::Automation]);
    let values: Vec<f64> = b.slices.iter().map(|s| s.value).collect();
    assert_eq!(values, [2.5, 1.25]);
}

/// Billing-cycle parts per million are percent of the cycle's allowance.
#[test]
fn test_billing_cycle_share_is_percent() {
    let b = parse(
        r#"{"billingCyclePpm":{"bySource":[{"source":"GROK_BOT_USAGE_SOURCE_GROUP","ppm":50000}]}}"#,
    )
    .unwrap();
    assert_eq!(b.unit, BreakdownUnit::BillingCyclePercent);
    assert_eq!(b.slices[0].value, 5.0);
}

/// No unit means no breakdown; an empty one has no slices and is hidden.
#[test]
fn test_missing_or_empty_breakdown() {
    assert!(parse("{}").is_none());
    let empty = parse(r#"{"weeklyLimitPpm":{}}"#).unwrap();
    assert!(empty.slices.is_empty());
    assert!(!empty.is_shown(Utc::now()));
}

/// Unspecified and unknown sources count as `Other`; the wire is matched
/// case-insensitively, by number and by bare name.
#[test]
fn test_unknown_and_unspecified_sources_count_as_other() {
    assert_eq!(
        Source::from_wire("GROK_BOT_USAGE_SOURCE_UNSPECIFIED"),
        Source::Other
    );
    assert_eq!(
        Source::from_wire("GROK_BOT_USAGE_SOURCE_SOMETHING_NEW"),
        Source::Other
    );
    assert_eq!(
        Source::from_wire("grok_bot_usage_source_agent"),
        Source::Agent
    );
    assert_eq!(Source::from_wire("4"), Source::Group);
    assert_eq!(Source::Agent.display_name(), "Bot to Bot");
    assert_eq!(Source::Automation.raw_value(), "automation");
}

/// Equal values keep the source order.
#[test]
fn test_ties_keep_source_order() {
    let b = parse(
        r#"{"weeklyLimitPpm":{"bySource":[{"source":"AGENT","ppm":10000},{"source":"USER","ppm":10000}]}}"#,
    )
    .unwrap();
    let sources: Vec<Source> = b.slices.iter().map(|s| s.source).collect();
    assert_eq!(sources, [Source::User, Source::Agent]);
}

/// A breakdown is hidden once its period has ended.
#[test]
fn test_hidden_after_the_period_ends() {
    let end = secs(1_000);
    let b = GrokbotUsageBreakdown {
        unit: BreakdownUnit::WeeklyLimitPercent,
        period_start: None,
        period_end: Some(end),
        slices: vec![tokenmon_provider_grokbot::BreakdownSlice {
            source: Source::User,
            value: 10.0,
        }],
    };
    assert!(b.is_shown(secs(999)));
    assert!(!b.is_shown(end));
}

/// The bearer token is the JWT inside `WorkosCursorSessionToken`.
#[test]
fn test_access_token_comes_from_the_session_cookie() {
    // Short fixtures: the repo's gitleaks rule flags realistic cookie values.
    let header = "other=1; WorkosCursorSessionToken=u1%3A%3Aa.b.c; theme=dark";
    assert_eq!(
        access_token_from_cookie_header(header).as_deref(),
        Some("a.b.c")
    );
    assert_eq!(
        access_token_from_cookie_header("WorkosCursorSessionToken=a.b.c").as_deref(),
        Some("a.b.c")
    );
}

/// No JWT, no token.
#[test]
fn test_no_access_token_without_a_jwt() {
    assert_eq!(
        access_token_from_cookie_header("WorkosCursorSessionToken=live"),
        None
    );
    assert_eq!(access_token_from_cookie_header("other=aaa.bbb.ccc"), None);
    assert_eq!(access_token_from_cookie_header(""), None);
    // A malformed percent escape is not a token either.
    assert_eq!(
        access_token_from_cookie_header("WorkosCursorSessionToken=%zz.b.c"),
        None
    );
}

fn client(server: &MockServer) -> GrokbotUsageClient {
    let http = CursorHttp::with_base_url(
        ProviderErrorContext::Grokbot,
        &server.uri(),
        Some("test"),
        Arc::new(FixedClock::from_unix(1_787_000_000)),
    )
    .unwrap();
    GrokbotUsageClient::new(http).with_rpc_base_url(&server.uri())
}

/// The call sends the cookie's JWT as a bearer token, the Connect protocol
/// header and an empty JSON body, and parses the reply.
#[tokio::test]
async fn test_fetch_breakdown_sends_bearer_token_and_parses_reply() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/aiserver.v1.DashboardService/GetGrokBotUsageBreakdown",
        ))
        .and(header("authorization", "Bearer a.b.c"))
        .and(header("connect-protocol-version", "1"))
        .and(body_json(serde_json::json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_string(WEEKLY_FIXTURE))
        .expect(1)
        .mount(&server)
        .await;
    let b = client(&server)
        .fetch_breakdown("WorkosCursorSessionToken=u1%3A%3Aa.b.c")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(b.slices.len(), 3);
}

/// Without a JWT in the cookie nothing is requested.
#[tokio::test]
async fn test_fetch_breakdown_without_a_token_makes_no_request() {
    let server = MockServer::start().await;
    let result = client(&server)
        .fetch_breakdown("WorkosCursorSessionToken=live")
        .await
        .unwrap();
    assert!(result.is_none());
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// A server error is an error for the caller (the poller keeps the last
/// breakdown and ignores it).
#[tokio::test]
async fn test_fetch_breakdown_server_error_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    assert!(
        client(&server)
            .fetch_breakdown("WorkosCursorSessionToken=u1%3A%3Aa.b.c")
            .await
            .is_err()
    );
}
