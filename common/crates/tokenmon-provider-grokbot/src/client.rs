//! The Grokbot fetch pipeline (spec 02 sections 2.1 and 2.2).
//!
//! [`GrokbotUsageClient`] performs the one POST. [`GrokbotFetcher`] binds it
//! to the **Cursor** cookie session: Grokbot rejections count against Cursor's
//! three-strike counter, except a non-challenge 403, which is a transient
//! "no access" error and never a strike.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde_json::json;
use tokenmon_core::{AuthSession, Clock, ProviderError, ProviderErrorContext, UsageFetcher};
use tokenmon_provider_cursor::CursorHttp;

use crate::breakdown::GrokbotUsageBreakdown;
use crate::models::GrokbotSnapshot;
use crate::parse::parse_usage_status;

const CTX: ProviderErrorContext = ProviderErrorContext::Grokbot;
/// Path of the Sand (Grok Bot) usage endpoint.
pub const USAGE_PATH: &str = "/api/dashboard/get-sand-usage-status";
/// `Referer` sent with the request.
pub const REFERER: &str = "https://cursor.com/bot";
/// Connect RPC host for the usage breakdown, which the dashboard's REST
/// wrapper does not expose.
pub const RPC_BASE_URL: &str = "https://api2.cursor.sh";
/// Path of the usage breakdown RPC.
pub const BREAKDOWN_PATH: &str = "/aiserver.v1.DashboardService/GetGrokBotUsageBreakdown";

const SESSION_COOKIE_PREFIX: &str = "workoscursorsessiontoken=";

/// Talks to `cursor.com` for the Grokbot allowance.
#[derive(Debug, Clone)]
pub struct GrokbotUsageClient {
    http: CursorHttp,
    rpc_base_url: String,
}

impl GrokbotUsageClient {
    /// A client over `http`.
    pub fn new(http: CursorHttp) -> Self {
        Self {
            http,
            rpc_base_url: RPC_BASE_URL.to_owned(),
        }
    }

    /// Replaces the RPC host (tests point it at a mock server).
    #[must_use]
    pub fn with_rpc_base_url(mut self, base_url: &str) -> Self {
        base_url.clone_into(&mut self.rpc_base_url);
        self
    }

    /// Production client for `https://cursor.com`; `app_version` feeds the
    /// `User-Agent`.
    ///
    /// # Errors
    /// `Network` when the TLS backend cannot be initialised.
    pub fn production(
        app_version: Option<&str>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, ProviderError> {
        Ok(Self::new(CursorHttp::production(CTX, app_version, clock)?))
    }

    /// `fetchUsageStatus`: POST `{}` and parse the reply.
    ///
    /// # Errors
    /// Per spec 02 section 2.2: transport gives `Network`, a challenge gives a
    /// transient `BadResponse`, a plain 403 gives the "not available" custom
    /// error, 401 and login pages give `Unauthorized`.
    pub async fn fetch_usage_status(
        &self,
        cookie: &str,
        account_email: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<GrokbotSnapshot, ProviderError> {
        let body = self
            .http
            .post_json(USAGE_PATH, cookie, REFERER, &json!({}))
            .await?;
        parse_usage_status(&body, account_email, now)
    }

    /// This period's usage by source (You, Routines, Bot to Bot, ...), or
    /// `None` when the session cookie holds no access token or the account
    /// has no breakdown.
    ///
    /// The RPC host authenticates with a bearer token rather than the cookie;
    /// the token is the one inside the session cookie.
    ///
    /// # Errors
    /// Transport, status and body errors, as for [`Self::fetch_usage_status`].
    pub async fn fetch_breakdown(
        &self,
        cookie: &str,
    ) -> Result<Option<GrokbotUsageBreakdown>, ProviderError> {
        let Some(token) = access_token_from_cookie_header(cookie) else {
            return Ok(None);
        };
        let body = self
            .http
            .post_json_bearer(
                &self.rpc_base_url,
                BREAKDOWN_PATH,
                &token,
                &[("Connect-Protocol-Version", "1")],
                &json!({}),
            )
            .await?;
        let root = tokenmon_core::net::json_object(&body, CTX)?;
        Ok(GrokbotUsageBreakdown::parse(&root))
    }
}

/// The access token inside `WorkosCursorSessionToken`, whose value is
/// `<user id>::<JWT>` (percent-encoded); `None` when the cookie is missing or
/// holds no JWT.
///
/// # Examples
///
/// ```
/// use tokenmon_provider_grokbot::access_token_from_cookie_header;
/// let header = "other=1; WorkosCursorSessionToken=u1%3A%3Aa.b.c; theme=dark";
/// assert_eq!(access_token_from_cookie_header(header).as_deref(), Some("a.b.c"));
/// assert_eq!(access_token_from_cookie_header("WorkosCursorSessionToken=live"), None);
/// ```
pub fn access_token_from_cookie_header(header: &str) -> Option<String> {
    let value = header.split(';').map(str::trim).find_map(|pair| {
        let head = pair.get(..SESSION_COOKIE_PREFIX.len())?;
        head.eq_ignore_ascii_case(SESSION_COOKIE_PREFIX)
            .then(|| &pair[SESSION_COOKIE_PREFIX.len()..])
    })?;
    let decoded = percent_decode(value)?;
    let token = decoded.rsplit("::").next().unwrap_or(&decoded);
    (token.split('.').count() == 3).then(|| token.to_owned())
}

/// Decodes `%XX` escapes; `None` for a malformed escape or non-UTF-8 result.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// [`UsageFetcher`] for Grokbot bound to the shared Cursor session.
pub struct GrokbotFetcher {
    client: GrokbotUsageClient,
    session: Arc<dyn AuthSession>,
    clock: Arc<dyn Clock>,
    last: Mutex<Option<GrokbotSnapshot>>,
}

impl GrokbotFetcher {
    /// Binds `client` to the Cursor `session`.
    pub fn new(
        client: GrokbotUsageClient,
        session: Arc<dyn AuthSession>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            client,
            session,
            clock,
            last: Mutex::new(None),
        }
    }

    /// Forgets the last snapshot (sign-out). History stores are untouched.
    pub fn clear_snapshot(&self) {
        *self.lock() = None;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<GrokbotSnapshot>> {
        self.last
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl UsageFetcher for GrokbotFetcher {
    type Snapshot = GrokbotSnapshot;

    async fn fetch(&self) -> Result<GrokbotSnapshot, ProviderError> {
        let cookie = self
            .session
            .cookie_header()
            .filter(|c| !c.is_empty())
            .ok_or(ProviderError::NotSignedIn(CTX))?;
        // The generation guard: a 401 that arrives after the user signed out
        // and in again belongs to the old cookie and must not strike the new
        // session.
        let state = self.session.state();
        let generation = state.session_generation;
        let result = self
            .client
            .fetch_usage_status(&cookie, state.account_email.as_deref(), self.clock.now())
            .await;
        match result {
            Ok(snapshot) => {
                self.session.record_auth_success();
                *self.lock() = Some(snapshot.clone());
                Ok(snapshot)
            }
            Err(error) => {
                if matches!(error, ProviderError::Unauthorized(_))
                    && self.session.is_current(generation)
                {
                    self.session
                        .record_auth_failure("Cursor rejected the session");
                }
                Err(error)
            }
        }
    }

    fn snapshot(&self) -> Option<GrokbotSnapshot> {
        self.lock().clone()
    }
}
