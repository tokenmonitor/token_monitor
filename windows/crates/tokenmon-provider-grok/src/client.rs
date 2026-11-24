//! Grok usage client (spec 02 sections 1.2.1 and 1.2.6).
//!
//! The gRPC-web billing endpoint behind grok.com Settings, Usage is the only
//! usage source; a failure backs off like every other provider (upstream
//! 8175790e dropped the guessed REST probes).
//!
//! HTTP goes through the small [`HttpTransport`] trait so the logic is tested
//! with a fake; [`ReqwestTransport`] is the production implementation (no
//! cookie jar, no cache).

use std::sync::Arc;
use std::time::Duration;

use tokenmon_core::usage::SavedResets;
use tokenmon_core::{BoxFuture, Clock, ProviderError, ProviderErrorContext, UsageError};

use crate::grpc::{self, Parsed};
use crate::models::{WeeklyUsageSnapshot, synthesize_products};

/// gRPC-web billing endpoint.
pub const BILLING_ENDPOINT: &str =
    "https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig";

/// gRPC-web endpoint listing saved usage-limit reset coupons.
pub const REMAINING_RESETS_ENDPOINT: &str =
    "https://grok.com/prod_mc_billing.ConsumerUiSvc/GetRemainingResets";

/// Per-request timeout.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Largest response body read; the real response is a few hundred bytes.
pub const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

const ORIGIN: &str = "https://grok.com";
const REFERER: &str = "https://grok.com/?_s=usage";
const CTX: ProviderErrorContext = ProviderErrorContext::Grok;

/// HTTP verb used by the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    /// `GET`.
    Get,
    /// `POST`.
    Post,
}

/// An outgoing request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// Verb.
    pub method: HttpMethod,
    /// Absolute URL.
    pub url: String,
    /// Header name and value pairs, in send order.
    pub headers: Vec<(String, String)>,
    /// Body bytes (empty for `GET`).
    pub body: Vec<u8>,
    /// Request timeout.
    pub timeout: Duration,
}

impl HttpRequest {
    /// The first header named `name`, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }
}

/// A response with the body fully read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Header pairs.
    pub headers: Vec<(String, String)>,
    /// Body bytes.
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// The first header named `name`, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Sends one request. Dropping the returned future cancels it; cancellation
/// is not an error (the poller treats a dropped fetch as skipped).
pub trait HttpTransport: Send + Sync {
    /// Performs the request; `Err` carries a short transport failure reason.
    fn send(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, String>>;
}

/// Production transport on `reqwest`: no cookie jar, no cache, bounded body.
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// Builds the transport.
    ///
    /// # Errors
    ///
    /// The TLS backend could not be initialized.
    pub fn new() -> Result<Self, reqwest::Error> {
        Ok(Self {
            client: reqwest::Client::builder().build()?,
        })
    }
}

impl HttpTransport for ReqwestTransport {
    fn send(&self, request: HttpRequest) -> BoxFuture<'_, Result<HttpResponse, String>> {
        Box::pin(async move {
            let method = match request.method {
                HttpMethod::Get => reqwest::Method::GET,
                HttpMethod::Post => reqwest::Method::POST,
            };
            let mut builder = self
                .client
                .request(method, &request.url)
                .timeout(request.timeout);
            for (name, value) in &request.headers {
                builder = builder.header(name, value);
            }
            if !request.body.is_empty() {
                builder = builder.body(request.body.clone());
            }
            let mut response = builder
                .send()
                .await
                .map_err(|e| e.without_url().to_string())?;
            let status = response.status().as_u16();
            let headers = response
                .headers()
                .iter()
                .filter_map(|(k, v)| Some((k.as_str().to_owned(), v.to_str().ok()?.to_owned())))
                .collect();
            let mut body = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|e| e.without_url().to_string())?
            {
                if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                    return Err("response too large".to_owned());
                }
                body.extend_from_slice(&chunk);
            }
            Ok(HttpResponse {
                status,
                headers,
                body,
            })
        })
    }
}

/// Bot-protection challenge, decided by [`tokenmon_core::net::is_bot_challenge`]
/// (a `cf-mitigated` header, or a 403 whose body looks like HTML).
pub fn is_bot_challenge(response: &HttpResponse) -> bool {
    let headers: reqwest::header::HeaderMap = response
        .headers
        .iter()
        .filter_map(|(k, v)| {
            Some((
                reqwest::header::HeaderName::from_bytes(k.as_bytes()).ok()?,
                reqwest::header::HeaderValue::from_str(v).ok()?,
            ))
        })
        .collect();
    let status = reqwest::StatusCode::from_u16(response.status).unwrap_or_default();
    tokenmon_core::net::is_bot_challenge(status, &headers, &response.body)
}

/// The transient error for a bot challenge.
pub fn bot_challenge_error(status: u16) -> ProviderError {
    let message = format!("Request blocked by a bot-protection check (HTTP {status}). Retrying.");
    ProviderError::Custom {
        usage: UsageError::BadResponse(message.clone()),
        message,
    }
}

/// Endpoint URLs; overridable for tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrokEndpoints {
    /// The gRPC-web billing URL.
    pub billing: String,
    /// The gRPC-web saved-resets URL.
    pub remaining_resets: String,
}

impl Default for GrokEndpoints {
    fn default() -> Self {
        Self {
            billing: BILLING_ENDPOINT.to_owned(),
            remaining_resets: REMAINING_RESETS_ENDPOINT.to_owned(),
        }
    }
}

/// Fetches SuperGrok weekly usage.
pub struct GrokUsageClient {
    transport: Arc<dyn HttpTransport>,
    clock: Arc<dyn Clock>,
    endpoints: GrokEndpoints,
    user_agent: String,
    cookie_header: Option<String>,
    account_email: Option<String>,
}

impl GrokUsageClient {
    /// A client sending `cookie_header` (`None` means not signed in) with the
    /// given `User-Agent` (`TokenMon/<version>`).
    pub fn new(
        transport: Arc<dyn HttpTransport>,
        clock: Arc<dyn Clock>,
        user_agent: impl Into<String>,
        cookie_header: Option<String>,
        account_email: Option<String>,
    ) -> Self {
        Self {
            transport,
            clock,
            endpoints: GrokEndpoints::default(),
            user_agent: user_agent.into(),
            cookie_header,
            account_email,
        }
    }

    /// Replaces the endpoint URLs (tests point them at a mock server).
    #[must_use]
    pub fn with_endpoints(mut self, endpoints: GrokEndpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    /// Fetches usage from the gRPC-web billing endpoint.
    ///
    /// # Errors
    ///
    /// `NotSignedIn` without a cookie header, `Unauthorized` when the endpoint
    /// rejects the session, otherwise the request or parse error.
    pub async fn fetch_usage(&self) -> Result<WeeklyUsageSnapshot, ProviderError> {
        if self.cookie_header.is_none() {
            return Err(ProviderError::NotSignedIn(CTX));
        }
        self.fetch_grpc_web_billing().await
    }

    /// Saved reset coupons that have not expired now, or `None` when there
    /// are none.
    ///
    /// # Errors
    ///
    /// `NotSignedIn` without a cookie header, otherwise any request, status or
    /// parse error. Callers treat every error as "no new information": this
    /// call never decides whether the session is valid.
    pub async fn fetch_saved_resets(&self) -> Result<Option<SavedResets>, ProviderError> {
        if self.cookie_header.is_none() {
            return Err(ProviderError::NotSignedIn(CTX));
        }
        let body = self.post_grpc_web(&self.endpoints.remaining_resets).await?;
        let expiries: Vec<_> = grpc::parse_reset_expiries(&body)?
            .into_iter()
            .map(Some)
            .collect();
        Ok(SavedResets::from_expirations(&expiries, self.clock.now()))
    }

    fn headers(&self, extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let cookie = self
            .cookie_header
            .iter()
            .filter(|c| !c.is_empty())
            .map(|c| ("Cookie".to_owned(), c.clone()));
        let extra = extra
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()));
        let tail = [
            ("Origin", ORIGIN),
            ("Referer", REFERER),
            ("User-Agent", self.user_agent.as_str()),
        ]
        .map(|(k, v)| (k.to_owned(), v.to_owned()));
        cookie.chain(extra).chain(tail).collect()
    }

    async fn fetch_grpc_web_billing(&self) -> Result<WeeklyUsageSnapshot, ProviderError> {
        let body = self.post_grpc_web(&self.endpoints.billing).await?;
        grpc::validate_trailers(&body)?;
        let now = self.clock.now();
        let parsed = grpc::parse_usage(&body, now)?;
        Ok(self.snapshot_from(parsed, now))
    }

    /// POSTs an empty gRPC-web message to `url` and returns the body once the
    /// HTTP status and any header `grpc-status` are clean. Trailer-frame
    /// status is left to the parser.
    async fn post_grpc_web(&self, url: &str) -> Result<Vec<u8>, ProviderError> {
        let request = HttpRequest {
            method: HttpMethod::Post,
            url: url.to_owned(),
            headers: self.headers(&[
                ("Content-Type", "application/grpc-web+proto"),
                ("x-grpc-web", "1"),
                ("x-user-agent", "connect-es/2.1.1"),
                ("Accept", "*/*"),
            ]),
            // Empty gRPC-web data frame: flag 0, big-endian length 0.
            body: vec![0, 0, 0, 0, 0],
            timeout: REQUEST_TIMEOUT,
        };
        let response = self
            .transport
            .send(request)
            .await
            .map_err(|message| ProviderError::Network(CTX, message))?;
        if is_bot_challenge(&response) {
            return Err(bot_challenge_error(response.status));
        }
        if matches!(response.status, 401 | 403) {
            return Err(ProviderError::Unauthorized(CTX));
        }
        if response.status != 200 {
            return Err(grpc::http_status_error(i64::from(response.status)));
        }
        // A trailers-only reply carries `grpc-status` in the HTTP headers.
        grpc::validate_headers(
            response.header("grpc-status"),
            response.header("grpc-message"),
        )?;
        Ok(response.body)
    }

    fn snapshot_from(
        &self,
        parsed: Parsed,
        now: chrono::DateTime<chrono::Utc>,
    ) -> WeeklyUsageSnapshot {
        let used = parsed.used_percent;
        let products = if parsed.products.is_empty() {
            synthesize_products(used)
        } else {
            parsed.products
        };
        WeeklyUsageSnapshot::new(now, used)
            .with_remaining((100.0 - used).max(0.0))
            .with_resets_at(parsed.resets_at)
            .with_products(products)
            .with_account(self.account_email.clone())
    }
}
