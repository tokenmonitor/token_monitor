//! claude.ai usage endpoint: request shape, response parsing and snapshot.

use chrono::{DateTime, Duration, Utc};
use serde_json::{Map, Value};
use tokenmon_core::usage::{SavedResets, UsagePool};
use tokenmon_core::util::parse_flexible;
use tokenmon_core::{ProviderError, ProviderErrorContext, UsageError};

const CTX: ProviderErrorContext = ProviderErrorContext::Claude;
const WEEK_SECONDS: i64 = 604_800;

/// Cookie name that carries the organization id (case-sensitive match).
pub const ORG_COOKIE_NAME: &str = "lastActiveOrg";
/// `Referer` sent with every usage request.
pub const REFERER: &str = "https://claude.ai/";

/// One claude.ai rate-limit window (`five_hour` / `seven_day`).
#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeUsageWindow {
    /// 0..=100 utilization.
    pub used_percent: f64,
    /// When the window resets, if reported.
    pub resets_at: Option<DateTime<Utc>>,
    /// Model the limit applies to (`Fable`) when model-scoped.
    pub scope_name: Option<String>,
}

impl ClaudeUsageWindow {
    /// `100 - used`, clamped to 0..=100.
    pub fn remaining_percent(&self) -> f64 {
        (100.0 - self.used_percent).clamp(0.0, 100.0)
    }
}

/// Parsed `/api/organizations/{org}/usage` payload.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClaudeUsageResponse {
    /// The 5-hour session window.
    pub five_hour: Option<ClaudeUsageWindow>,
    /// The weekly window.
    pub seven_day: Option<ClaudeUsageWindow>,
    /// Saved "Reset for free" resets, from the `cedar_ember` block.
    pub saved_resets: Option<SavedResets>,
}

/// `JSON.number`: a number (not a bool), a numeric string, or an object with
/// `val` / `value` (recursively). NaN and infinities are rejected.
fn json_number(v: Option<&Value>) -> Option<f64> {
    let n = match v? {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().parse::<f64>().ok()?,
        Value::Object(o) => return json_number(o.get("val").or_else(|| o.get("value"))),
        _ => return None,
    };
    n.is_finite().then_some(n)
}

fn json_string(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::to_owned)
}

fn reset_date(v: Option<&Value>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(v?.as_str()?)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn window(raw: Option<&Value>) -> Option<ClaudeUsageWindow> {
    let obj = raw?.as_object()?;
    Some(ClaudeUsageWindow {
        used_percent: json_number(obj.get("utilization"))?.clamp(0.0, 100.0),
        resets_at: reset_date(obj.get("resets_at")),
        scope_name: None,
    })
}

fn scope_name(raw: Option<&Value>) -> Option<String> {
    let model = raw?.as_object()?.get("model")?.as_object()?;
    json_string(model.get("display_name")).or_else(|| json_string(model.get("id")))
}

fn rank(entry: &Map<String, Value>, percent: f64) -> (u8, u8, f64) {
    let unscoped = u8::from(scope_name(entry.get("scope")).is_none());
    let active = u8::from(entry.get("is_active") == Some(&Value::Bool(true)));
    (unscoped, active, percent)
}

fn better(a: (u8, u8, f64), b: (u8, u8, f64)) -> bool {
    (a.0, a.1).cmp(&(b.0, b.1)).then(a.2.total_cmp(&b.2)) == std::cmp::Ordering::Greater
}

/// Best candidate so far: entry, percent, rank.
type Best<'a> = (&'a Map<String, Value>, f64, (u8, u8, f64));

/// Picks the `limits` entry for `group`: unscoped first, then active, then
/// the most used; the first entry wins ties.
fn limit_window(limits: &[&Map<String, Value>], group: &str) -> Option<ClaudeUsageWindow> {
    let mut best: Option<Best<'_>> = None;
    for entry in limits {
        if entry.get("group").and_then(Value::as_str) != Some(group) {
            continue;
        }
        let Some(percent) = json_number(entry.get("percent")) else {
            continue;
        };
        let r = rank(entry, percent);
        if best.as_ref().is_none_or(|(_, _, br)| better(r, *br)) {
            best = Some((entry, percent, r));
        }
    }
    best.map(|(entry, percent, _)| ClaudeUsageWindow {
        used_percent: percent.clamp(0.0, 100.0),
        resets_at: reset_date(entry.get("resets_at")),
        scope_name: scope_name(entry.get("scope")),
    })
}

fn looks_like_html(data: &[u8]) -> bool {
    let head = &data[..data.len().min(512)];
    std::str::from_utf8(head).is_ok_and(|s| s.trim_start().starts_with('<'))
}

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

const CHALLENGE: [&str; 6] = [
    "cf-chl",
    "challenge-platform",
    "cf_chl_opt",
    "just a moment...",
    "attention required! | cloudflare",
    "cf-browser-verification",
];
const LOGIN: [&str; 10] = [
    "type=\"password\"",
    "type='password'",
    "sign in",
    "sign-in",
    "signin",
    "log in",
    "log-in",
    "login",
    "authkit",
    "workos",
];

fn looks_like_login_page(data: &[u8]) -> bool {
    if !looks_like_html(data) {
        return false;
    }
    let lower = data[..data.len().min(65_536)].to_ascii_lowercase();
    !CHALLENGE.iter().any(|m| contains(&lower, m)) && LOGIN.iter().any(|m| contains(&lower, m))
}

/// Saved-reset grants in the usage payload's `cedar_ember` block:
/// `{"eligible": true, "grants": [{"resets_left": 1, "resets_total": 1,
/// "paused": false, "starts_at": "...", "ends_at": "..."}]}`.
///
/// Grant ids are not read. A grant with an unreadable count, pause state or
/// bound is dropped without hiding the others.
mod grants {
    use super::{DateTime, SavedResets, Utc, Value, json_number, parse_flexible};

    /// Resets available at `now`, or `None` when the block is absent, the
    /// account is not eligible, or no grant offers a reset.
    pub(super) fn saved_resets(raw: Option<&Value>, now: DateTime<Utc>) -> Option<SavedResets> {
        let block = raw?.as_object()?;
        if block.get("eligible") != Some(&Value::Bool(true)) {
            return None;
        }
        let grants = block.get("grants")?.as_array()?;
        let mut expirations: Vec<Option<DateTime<Utc>>> = Vec::new();
        for grant in grants.iter().filter_map(Value::as_object) {
            let Some((count, expiry)) = available_resets(grant, now) else {
                continue;
            };
            if count > SavedResets::MAXIMUM_COUNT as usize - expirations.len() {
                return None;
            }
            expirations.extend(std::iter::repeat_n(expiry, count));
        }
        SavedResets::from_expirations(&expirations, now)
    }

    /// Resets one grant offers at `now` and their expiry; `None` for a
    /// malformed grant. A paused, used-up, not-yet-started or expired grant
    /// offers zero.
    fn available_resets(
        grant: &serde_json::Map<String, Value>,
        now: DateTime<Utc>,
    ) -> Option<(usize, Option<DateTime<Utc>>)> {
        let left = json_number(grant.get("resets_left"))?;
        if left.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&left) {
            return None;
        }
        let paused = grant.get("paused")?.as_bool()?;
        let starts_at = bound(grant.get("starts_at")).ok()?;
        let ends_at = bound(grant.get("ends_at")).ok()?;
        if json_number(grant.get("resets_total")).is_some_and(|total| left > total) {
            return None;
        }
        if paused || starts_at.is_some_and(|s| s > now) || ends_at.is_some_and(|e| e <= now) {
            return Some((0, None));
        }
        Some((left as usize, ends_at))
    }

    /// A grant bound that could not be read.
    struct Malformed;

    /// An absent or null bound is open (`Ok(None)`); an unreadable one is
    /// malformed.
    fn bound(raw: Option<&Value>) -> Result<Option<DateTime<Utc>>, Malformed> {
        match raw {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => parse_flexible(s).map(Some).ok_or(Malformed),
            Some(_) => Err(Malformed),
        }
    }
}

/// Parses the raw usage body (`ProviderHTTP.jsonObject` then
/// `ClaudeUsageResponse.parse`), judging saved-reset grants against the
/// current time.
///
/// See [`parse_usage_body_at`] for the rules and errors.
///
/// # Errors
///
/// As [`parse_usage_body_at`].
pub fn parse_usage_body(data: &[u8]) -> Result<ClaudeUsageResponse, ProviderError> {
    parse_usage_body_at(data, Utc::now())
}

/// Parses the raw usage body; `now` decides which saved-reset grants are
/// currently available.
///
/// The top-level `five_hour` / `seven_day` objects win; accounts whose
/// `seven_day` is `null` report the weekly pool only in `limits`
/// (`group: "weekly"`).
///
/// # Errors
///
/// A login page is [`ProviderError::Unauthorized`]; other HTML is a
/// `BadResponse("Unexpected HTML response")`; any other non-object body is
/// `BadResponse("Malformed response body")`.
///
/// # Examples
///
/// ```
/// use tokenmon_provider_claude::parse_usage_body;
/// let r = parse_usage_body(br#"{"five_hour":{"utilization":130,"resets_at":"2026-08-21T14:00:00.127279+00:00"}}"#).unwrap();
/// assert_eq!(r.five_hour.unwrap().used_percent, 100.0);
/// assert!(parse_usage_body(b"not json").is_err());
/// ```
pub fn parse_usage_body_at(
    data: &[u8],
    now: DateTime<Utc>,
) -> Result<ClaudeUsageResponse, ProviderError> {
    let root = match serde_json::from_slice::<Value>(data) {
        Ok(Value::Object(o)) => o,
        _ if looks_like_login_page(data) => return Err(ProviderError::Unauthorized(CTX)),
        _ if looks_like_html(data) => {
            return Err(ProviderError::BadResponse(
                CTX,
                "Unexpected HTML response".to_owned(),
            ));
        }
        _ => {
            return Err(ProviderError::BadResponse(
                CTX,
                "Malformed response body".to_owned(),
            ));
        }
    };
    let limits: Vec<&Map<String, Value>> = match root.get("limits") {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_object).collect(),
        _ => Vec::new(),
    };
    Ok(ClaudeUsageResponse {
        five_hour: window(root.get("five_hour")).or_else(|| limit_window(&limits, "session")),
        seven_day: window(root.get("seven_day")).or_else(|| limit_window(&limits, "weekly")),
        saved_resets: grants::saved_resets(root.get("cedar_ember"), now),
    })
}

/// Extracts the org UUID from the `lastActiveOrg` cookie pair.
///
/// The cookie name match is case-sensitive; the first match wins; an empty
/// value yields `None`. The value is returned verbatim (never empty).
///
/// # Examples
///
/// ```
/// use tokenmon_provider_claude::organization_id_from_cookie_header;
/// assert_eq!(organization_id_from_cookie_header("a=b; lastActiveOrg=xyz"), Some("xyz".into()));
/// assert_eq!(organization_id_from_cookie_header("sessionKey=abc"), None);
/// ```
pub fn organization_id_from_cookie_header(header: &str) -> Option<String> {
    for pair in header.split(';') {
        // Swift `split(maxSplits: 1)` drops empty pieces: `=x` and `name=`
        // are one-part pairs and skipped.
        let Some((name, raw_value)) = pair.split_once('=') else {
            continue;
        };
        if name.is_empty() || raw_value.is_empty() {
            continue;
        }
        if name.trim_matches([' ', '\t']) == ORG_COOKIE_NAME {
            let value = raw_value.trim_matches([' ', '\t']);
            return (!value.is_empty()).then(|| value.to_owned());
        }
    }
    None
}

/// The usage request, ready for the HTTP wrapper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeUsageRequest {
    /// `GET https://claude.ai/api/organizations/{org}/usage?cedar_ember=1`
    /// (`cedar_ember=1` asks the endpoint to include the saved-reset grants).
    pub url: String,
    /// The same endpoint as a path under [`crate::fetch::DEFAULT_BASE_URL`].
    pub path: String,
    /// Stored (pruned) cookie header.
    pub cookie_header: String,
    /// [`REFERER`].
    pub referer: &'static str,
}

/// Builds the usage request for the org in `cookie_header`.
///
/// # Errors
///
/// Without an org id: a custom error whose usage kind is `NotSignedIn`
/// (counts as an auth-failure strike).
pub fn usage_request(cookie_header: &str) -> Result<ClaudeUsageRequest, ProviderError> {
    let org =
        organization_id_from_cookie_header(cookie_header).ok_or_else(|| ProviderError::Custom {
            message: "Claude organization id not found in session. Sign in again.".to_owned(),
            usage: UsageError::NotSignedIn,
        })?;
    Ok(ClaudeUsageRequest {
        url: format!("https://claude.ai/api/organizations/{org}/usage?cedar_ember=1"),
        path: format!("/api/organizations/{org}/usage?cedar_ember=1"),
        cookie_header: cookie_header.to_owned(),
        referer: REFERER,
    })
}

/// Headline Claude rate-limit snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeSnapshot {
    /// When the data was fetched.
    pub fetched_at: DateTime<Utc>,
    /// 5-hour window.
    pub five_hour: Option<ClaudeUsageWindow>,
    /// Weekly window.
    pub seven_day: Option<ClaudeUsageWindow>,
    /// Signed-in account email at publish time.
    pub account_email: Option<String>,
    /// Saved "Reset for free" resets, when the account has any.
    pub saved_resets: Option<SavedResets>,
}

impl ClaudeSnapshot {
    /// `fiveHour.resetsAt ?? sevenDay.resetsAt`.
    pub fn resets_at(&self) -> Option<DateTime<Utc>> {
        self.five_hour
            .as_ref()
            .and_then(|w| w.resets_at)
            .or_else(|| self.seven_day.as_ref().and_then(|w| w.resets_at))
    }

    /// `Weekly`, or `Weekly · <model>` (U+00B7) when the weekly limit is
    /// model-scoped.
    pub fn weekly_label(&self) -> String {
        match self.seven_day.as_ref().and_then(|w| w.scope_name.as_ref()) {
            Some(s) => format!("Weekly \u{b7} {s}"),
            None => "Weekly".to_owned(),
        }
    }

    /// `(fiveHour ?? sevenDay).usedPercent ?? 0`.
    pub fn headline_used_percent(&self) -> f64 {
        self.five_hour
            .as_ref()
            .or(self.seven_day.as_ref())
            .map_or(0.0, |w| w.used_percent)
    }

    /// One window gives its cadence; zero or two give `Mixed`.
    pub fn usage_pool(&self) -> UsagePool {
        let pools: Vec<UsagePool> = [
            self.five_hour.as_ref().map(|_| UsagePool::Hourly),
            self.seven_day.as_ref().map(|_| UsagePool::Weekly),
        ]
        .into_iter()
        .flatten()
        .collect();
        UsagePool::combining(&pools)
    }
}

/// Carries a past weekly reset forward by whole weeks until it is after
/// `now`; `r == now` gives `r + 1 week`.
///
/// # Examples
///
/// ```
/// use chrono::{TimeZone, Utc};
/// use tokenmon_provider_claude::project_weekly_reset;
/// let r = Utc.with_ymd_and_hms(2026, 9, 13, 8, 0, 0).unwrap();
/// let now = Utc.with_ymd_and_hms(2026, 10, 10, 9, 0, 0).unwrap();
/// assert_eq!(project_weekly_reset(Some(r), now), Some(Utc.with_ymd_and_hms(2026, 10, 11, 8, 0, 0).unwrap()));
/// ```
pub fn project_weekly_reset(r: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let r = r?;
    if r > now {
        return Some(r);
    }
    let weeks = (now - r).num_seconds().div_euclid(WEEK_SECONDS) + 1;
    Duration::try_seconds(weeks.checked_mul(WEEK_SECONDS)?).and_then(|d| r.checked_add_signed(d))
}
