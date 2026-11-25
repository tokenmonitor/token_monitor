//! Edge cases of the OpenCode console client: status mapping, workspace
//! fallbacks and the failure messages, plus the poller's console seam.
#![allow(
    clippy::unwrap_used,
    clippy::assert_is_empty,
    clippy::unnecessary_wraps
)]

use std::sync::Arc;

use tokenmon_core::{BoxFuture, ProviderError};
use tokenmon_provider_opencode::console::{
    ConsoleClient, ConsoleRequest, ConsoleResponse, ConsoleTransport, parse_go_meters,
};
use tokenmon_provider_opencode::poller::console_fetch_fn;

type Script = dyn Fn(&ConsoleRequest) -> Result<ConsoleResponse, String> + Send + Sync;

struct Scripted(Box<Script>);

impl ConsoleTransport for Scripted {
    fn get<'a>(
        &'a self,
        request: &'a ConsoleRequest,
    ) -> BoxFuture<'a, Result<ConsoleResponse, String>> {
        let r = (self.0)(request);
        Box::pin(async move { r })
    }
}

fn reply(status: u16, body: &str) -> Result<ConsoleResponse, String> {
    Ok(ConsoleResponse {
        status,
        final_path: "/console/api/x".to_owned(),
        cf_mitigated: false,
        is_html: false,
        body: body.as_bytes().to_vec(),
    })
}

const SEATED: &str =
    r#"{"access":{"meters":{"month":{"limitMicroCents":"6000000000","usedMicroCents":"1"}}}}"#;
const NO_SEAT: &str = r#"{"access":{"meters":{}}}"#;

fn client(
    f: impl Fn(&ConsoleRequest) -> Result<ConsoleResponse, String> + Send + Sync + 'static,
) -> ConsoleClient<Scripted> {
    ConsoleClient::new(Scripted(Box::new(f)))
}

fn msg(e: &ProviderError) -> String {
    match e {
        ProviderError::BadResponse(_, m) => m.clone(),
        other => other.to_string(),
    }
}

/// Any single meter makes the account a seat holder.
#[test]
fn test_has_any_for_each_single_meter() {
    let one = |name: &str| {
        format!(
            r#"{{"access":{{"meters":{{"{name}":{{"limitMicroCents":"100","usedMicroCents":"1"}}}}}}}}"#
        )
    };
    for name in ["fiveHour", "week", "month"] {
        assert!(
            parse_go_meters(one(name).as_bytes()).unwrap().has_any(),
            "{name}"
        );
    }
    assert!(!parse_go_meters(NO_SEAT.as_bytes()).unwrap().has_any());
}

/// A body without an `access` or `meters` object yields no meters rather
/// than an error.
#[test]
fn test_missing_access_or_meters_means_no_meters() {
    assert!(!parse_go_meters(br#"{"other":1}"#).unwrap().has_any());
    assert!(
        !parse_go_meters(br#"{"access":{"endsAt":"2026-08-01T00:00:00Z"}}"#)
            .unwrap()
            .has_any()
    );
    assert!(
        !parse_go_meters(br#"{"access":{"meters":[]}}"#)
            .unwrap()
            .has_any()
    );
}

/// HTTP 401 and a workspace-less 403 are session errors; other 4xx are a
/// plain bad response.
#[tokio::test]
async fn test_status_codes_map_to_errors() {
    let c = client(|_| reply(401, ""));
    assert!(matches!(
        c.list_orgs("cookie").await,
        Err(ProviderError::Unauthorized(_))
    ));
    let c = client(|_| reply(403, ""));
    assert!(matches!(
        c.list_orgs("cookie").await,
        Err(ProviderError::Unauthorized(_))
    ));
    let c = client(|_| reply(500, ""));
    assert_eq!(msg(&c.list_orgs("cookie").await.unwrap_err()), "HTTP 500");
    let c = client(|_| Err("offline".to_owned()));
    assert!(matches!(
        c.list_orgs("cookie").await,
        Err(ProviderError::Network(..))
    ));
}

/// A preferred workspace that holds no seat or is forbidden falls through to
/// the listed ones; the first seated workspace wins.
#[tokio::test]
async fn test_preferred_workspace_falls_back_to_listed_ones() {
    let c = client(|r| match (r.path.as_str(), r.org_id.as_deref()) {
        ("/console/api/orgs", _) => reply(200, r#"[{"id":"wrk_a"},{"id":"wrk_b"},{"id":"nope"}]"#),
        ("/console/api/go/status", Some("wrk_pref")) => reply(403, ""),
        ("/console/api/go/status", Some("wrk_a")) => reply(200, NO_SEAT),
        ("/console/api/go/status", Some("wrk_b")) => reply(200, SEATED),
        _ => Err("unexpected".to_owned()),
    });
    let (snap, org) = c
        .fetch_go_usage_snapshot("cookie", Some("wrk_pref"))
        .await
        .unwrap();
    assert_eq!(org, "wrk_b");
    assert!(!snap.windows.is_empty());
}

/// A seated preferred workspace is used directly; a non-`wrk_` hint is
/// ignored in favour of the list.
#[tokio::test]
async fn test_seated_preferred_workspace_and_invalid_hint() {
    let c = client(|r| match (r.path.as_str(), r.org_id.as_deref()) {
        ("/console/api/go/status", Some(_)) => reply(200, SEATED),
        ("/console/api/orgs", _) => reply(200, r#"[{"id":"wrk_listed"}]"#),
        _ => Err("unexpected".to_owned()),
    });
    let (_, org) = c
        .fetch_go_usage_snapshot("c", Some("wrk_known"))
        .await
        .unwrap();
    assert_eq!(org, "wrk_known");
    let (_, org) = c.fetch_go_usage_snapshot("c", Some("bogus")).await.unwrap();
    assert_eq!(org, "wrk_listed");
    // The preferred workspace is not tried a second time when it is listed.
    let (_, org) = c.fetch_go_usage_snapshot("c", None).await.unwrap();
    assert_eq!(org, "wrk_listed");
}

/// The three failure messages: no workspace, every workspace forbidden, and
/// no seat.
#[tokio::test]
async fn test_failure_messages_for_empty_forbidden_and_unseated_accounts() {
    let none = client(|_| reply(200, "[]"));
    assert_eq!(
        msg(&none.fetch_go_usage_snapshot("c", None).await.unwrap_err()),
        "No OpenCode workspace on this account."
    );
    let forbidden = client(|r| {
        if r.path == "/console/api/orgs" {
            reply(200, r#"[{"id":"wrk_a"},{"id":"wrk_b"}]"#)
        } else {
            reply(403, "")
        }
    });
    assert_eq!(
        msg(&forbidden
            .fetch_go_usage_snapshot("c", None)
            .await
            .unwrap_err()),
        "The OpenCode console denied access to this workspace."
    );
    let unseated = client(|r| {
        if r.path == "/console/api/orgs" {
            reply(200, r#"[{"id":"wrk_a"},{"id":"wrk_b"}]"#)
        } else if r.org_id.as_deref() == Some("wrk_a") {
            reply(403, "")
        } else {
            reply(200, NO_SEAT)
        }
    });
    assert!(
        msg(&unseated
            .fetch_go_usage_snapshot("c", None)
            .await
            .unwrap_err())
        .starts_with("No Go subscription")
    );
}

/// Workspace resolution prefers a valid hint, else the first listed one, and
/// reports an empty account.
#[tokio::test]
async fn test_resolve_org_id() {
    let c = client(|_| reply(200, r#"[{"id":"wrk_first"},{"id":"wrk_second"}]"#));
    assert_eq!(
        c.resolve_org_id("c", Some("wrk_hint")).await.unwrap(),
        "wrk_hint"
    );
    assert_eq!(
        c.resolve_org_id("c", Some("bad")).await.unwrap(),
        "wrk_first"
    );
    assert_eq!(c.resolve_org_id("c", None).await.unwrap(), "wrk_first");
    let empty = client(|_| reply(200, "[]"));
    assert_eq!(
        msg(&empty.resolve_org_id("c", None).await.unwrap_err()),
        "No OpenCode workspace on this account."
    );
}

/// The poller's console seam forwards the cookie and the known workspace to
/// the client and returns the served workspace.
#[tokio::test]
async fn test_console_fetch_fn_forwards_cookie_and_workspace() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let c = Arc::new(client(move |r| {
        sink.lock()
            .unwrap()
            .push((r.cookie.clone(), r.org_id.clone()));
        reply(200, SEATED)
    }));
    let fetch = console_fetch_fn(c);
    let (snap, org) = fetch("the-cookie".to_owned(), Some("wrk_x".to_owned()))
        .await
        .unwrap();
    assert_eq!(org, "wrk_x");
    assert!(
        snap.window(tokenmon_provider_opencode::snapshot::WindowKind::Monthly)
            .is_some()
    );
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        [("the-cookie".to_owned(), Some("wrk_x".to_owned()))]
    );
}
