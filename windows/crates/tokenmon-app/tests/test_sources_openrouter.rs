//! The OpenRouter typed source over a poller fed by a scripted endpoint fetcher.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{env, query};
use tokenmon_app::sources::{SnapshotView, openrouter_source};
use tokenmon_core::settings::Settings;
use tokenmon_core::{BoxFuture, ProviderError, ProviderUsagePoller};
use tokenmon_provider_openrouter::{
    ClientFactory, Endpoint, EndpointFetcher, NoCostEstimator, OpenRouterUsageClient,
    OpenRouterUsagePoller,
};
use tokenmon_ui::panels::common::AllowanceNoun;

struct Scripted(&'static str);

impl EndpointFetcher for Scripted {
    fn fetch(&self, endpoint: Endpoint) -> BoxFuture<'_, Result<Vec<u8>, ProviderError>> {
        let reset = self.0;
        Box::pin(async move {
            let body = match endpoint {
                Endpoint::Key => format!(
                    r#"{{"data":{{"usage":25,"limit":100,"limit_remaining":75,
                    "limit_reset":"{reset}","is_management_key":true}}}}"#
                ),
                // Account credits outrank the key limit, so withhold them to chart the limit.
                Endpoint::Credits => {
                    return Err(ProviderError::Network(
                        tokenmon_core::ProviderErrorContext::OpenRouter,
                        "no credits".into(),
                    ));
                }
                Endpoint::Activity => r#"{"data":[{"date":"2026-10-13 00:00:00","model":"m/x",
                    "usage":2.5,"requests":3,"prompt_tokens":10,"completion_tokens":5,
                    "reasoning_tokens":0}]}"#
                    .into(),
            };
            Ok(body.into_bytes())
        })
    }
}

fn poller(env: &common::Env, reset: &'static str) -> Arc<OpenRouterUsagePoller> {
    let factory: ClientFactory = Arc::new(move |_key| {
        Ok(OpenRouterUsageClient::new(
            Arc::new(Scripted(reset)),
            Arc::new(NoCostEstimator),
        ))
    });
    OpenRouterUsagePoller::new(
        Arc::clone(&env.sessions.openrouter),
        env.dyn_clock(),
        factory,
        Arc::new(|| true),
        Arc::new(|_| Some(Duration::from_secs(3600))),
    )
}

/// Signed out, the OpenRouter view has no snapshot.
#[tokio::test]
async fn test_openrouter_source_is_empty_without_a_snapshot() {
    let e = env();
    let source = openrouter_source(&poller(&e, "monthly"));
    let clock = e.dyn_clock();
    assert_eq!(
        source(&query(clock.as_ref(), 0, Settings::default())),
        SnapshotView::OpenRouter(None)
    );
}

/// A key with a monthly limit charts against the monthly allowance.
#[tokio::test]
async fn test_openrouter_source_charts_a_monthly_key_limit() {
    let e = env();
    e.sessions
        .openrouter
        .save_api_key("sk-or-v1-test")
        .expect("key stored");
    let p = poller(&e, "monthly");
    p.refresh_now().await;
    let source = openrouter_source(&p);
    let clock = e.dyn_clock();
    let SnapshotView::OpenRouter(Some(input)) =
        source(&query(clock.as_ref(), 0, Settings::default()))
    else {
        panic!("snapshot expected");
    };
    let daily = input.daily.expect("daily chart for a limited key");
    assert_eq!(daily.noun, AllowanceNoun::Monthly);
}

/// A key with a weekly limit charts against the weekly allowance.
#[tokio::test]
async fn test_openrouter_source_charts_a_weekly_key_limit() {
    let e = env();
    e.sessions
        .openrouter
        .save_api_key("sk-or-v1-test")
        .expect("key stored");
    let p = poller(&e, "weekly");
    p.refresh_now().await;
    let source = openrouter_source(&p);
    let clock = e.dyn_clock();
    let SnapshotView::OpenRouter(Some(input)) =
        source(&query(clock.as_ref(), 0, Settings::default()))
    else {
        panic!("snapshot expected");
    };
    let daily = input.daily.expect("daily chart for a limited key");
    assert_eq!(daily.noun, AllowanceNoun::Weekly);
}
