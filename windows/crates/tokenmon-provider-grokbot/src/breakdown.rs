//! The Grokbot usage breakdown by source (upstream 2ec2b515): parsing of
//! `aiserver.v1.DashboardService/GetGrokBotUsageBreakdown`.
//!
//! The response's `breakdown` oneof holds exactly one unit (a share of the
//! weekly allowance, of the billing cycle's, or billed cents) and one row per
//! UTC day and source. Rows are summed over the period. The parser never
//! panics on any input.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use tokenmon_core::ProviderId;
use tokenmon_core::Srgb;
use tokenmon_core::util::json::{first_double, number, string};

/// Who or what spent a share of the Grok Bot allowance, as the Grok Bot app
/// labels it. Declaration order is the tie-break order of the slices.
///
/// # Examples
///
/// ```
/// use tokenmon_provider_grokbot::GrokbotUsageSource;
/// assert_eq!(GrokbotUsageSource::from_wire("GROK_BOT_USAGE_SOURCE_AGENT"), GrokbotUsageSource::Agent);
/// assert_eq!(GrokbotUsageSource::Agent.display_name(), "Bot to Bot");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GrokbotUsageSource {
    /// The user's own chats ("You").
    User,
    /// Scheduled routines.
    Automation,
    /// Bot to bot calls.
    Agent,
    /// Group chats.
    Group,
    /// Anything else, including unspecified and unknown values.
    Other,
}

impl GrokbotUsageSource {
    /// Every source, in tie-break order.
    pub const ALL: [Self; 5] = [
        Self::User,
        Self::Automation,
        Self::Agent,
        Self::Group,
        Self::Other,
    ];

    /// Reads a protobuf JSON enum value (`GROK_BOT_USAGE_SOURCE_USER`, the
    /// bare `USER`, or its number). Unspecified and unknown values count as
    /// [`Self::Other`].
    pub fn from_wire(wire: &str) -> Self {
        let name = wire.to_uppercase().replace("GROK_BOT_USAGE_SOURCE_", "");
        match name.as_str() {
            "USER" | "1" => Self::User,
            "AUTOMATION" | "2" => Self::Automation,
            "AGENT" | "3" => Self::Agent,
            "GROUP" | "4" => Self::Group,
            _ => Self::Other,
        }
    }

    /// The stable lowercase name (the Swift raw value).
    pub fn raw_value(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Automation => "automation",
            Self::Agent => "agent",
            Self::Group => "group",
            Self::Other => "other",
        }
    }

    /// The Grok Bot app's label for the source.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::User => "You",
            Self::Automation => "Routines",
            Self::Agent => "Bot to Bot",
            Self::Group => "Group chats",
            Self::Other => "Other",
        }
    }

    /// Muted cool tones around the Grokbot slate accent, so the breakdown
    /// reads as part of the Grokbot panel. `User` is the accent itself; the
    /// others stay mid-tone so each shows against the empty track in both
    /// light and dark appearances.
    pub fn color(self) -> Srgb {
        match self {
            Self::User => ProviderId::Grokbot.accent(),
            Self::Automation => Srgb::rgb(0.40, 0.56, 0.82),
            Self::Agent => Srgb::rgb(0.42, 0.68, 0.70),
            Self::Group => Srgb::rgb(0.60, 0.52, 0.78),
            Self::Other => Srgb::rgb(0.62, 0.62, 0.64),
        }
    }
}

/// What a source's value measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakdownUnit {
    /// Percent of the weekly allowance (`weekly_limit_ppm`).
    WeeklyLimitPercent,
    /// Percent of the billing cycle's allowance (`billing_cycle_ppm`).
    BillingCyclePercent,
    /// Billed US dollars (`billed_cents`), for usage billed on demand.
    Usd,
}

/// One source's share of the period.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreakdownSlice {
    /// The source.
    pub source: GrokbotUsageSource,
    /// Percent of the allowance, or US dollars for [`BreakdownUnit::Usd`].
    pub value: f64,
}

/// How the Grok Bot allowance was spent this period, by source.
#[derive(Debug, Clone, PartialEq)]
pub struct GrokbotUsageBreakdown {
    /// What the slice values measure.
    pub unit: BreakdownUnit,
    /// Start of the period, when reported.
    pub period_start: Option<DateTime<Utc>>,
    /// End of the period, when reported.
    pub period_end: Option<DateTime<Utc>>,
    /// Sources that used anything, most used first; ties keep source order.
    pub slices: Vec<BreakdownSlice>,
}

/// The response's units: the `breakdown` oneof holds exactly one of these.
const UNIT_KEYS: [(BreakdownUnit, [&str; 2], &str); 3] = [
    (
        BreakdownUnit::WeeklyLimitPercent,
        ["weeklyLimitPpm", "weekly_limit_ppm"],
        "ppm",
    ),
    (
        BreakdownUnit::BillingCyclePercent,
        ["billingCyclePpm", "billing_cycle_ppm"],
        "ppm",
    ),
    (BreakdownUnit::Usd, ["billedCents", "billed_cents"], "cents"),
];

impl GrokbotUsageBreakdown {
    /// Sum of every source's value.
    pub fn total(&self) -> f64 {
        self.slices.iter().map(|s| s.value).sum()
    }

    /// True while there is usage to show for a period that has not ended.
    pub fn is_shown(&self, now: DateTime<Utc>) -> bool {
        !self.slices.is_empty() && self.period_end.is_none_or(|end| now < end)
    }

    /// Parses the response body; `None` when it carries no breakdown.
    ///
    /// Rows arrive per UTC day and source; they are summed over the period.
    /// Parts per million become percent and cents become dollars.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokenmon_provider_grokbot::GrokbotUsageBreakdown;
    /// let root = serde_json::json!({"weeklyLimitPpm": {"bySource": [{"source": "USER", "ppm": 250000}]}});
    /// let breakdown = GrokbotUsageBreakdown::parse(root.as_object().unwrap()).unwrap();
    /// assert_eq!(breakdown.slices[0].value, 25.0);
    /// ```
    pub fn parse(root: &Map<String, Value>) -> Option<Self> {
        UNIT_KEYS.iter().find_map(|(unit, keys, value_key)| {
            let splits = keys
                .iter()
                .find_map(|k| root.get(*k).and_then(Value::as_object))?;
            let rows = splits
                .get("bySource")
                .or_else(|| splits.get("by_source"))
                .and_then(Value::as_array)
                .map_or(&[][..], Vec::as_slice);
            let mut totals: BTreeMap<GrokbotUsageSource, f64> = BTreeMap::new();
            for row in rows.iter().filter_map(Value::as_object) {
                let raw = first_double(row, &[value_key]).unwrap_or(0.0);
                let scaled = if *unit == BreakdownUnit::Usd {
                    raw / 100.0
                } else {
                    raw / 10_000.0
                };
                *totals.entry(source_of(row)).or_insert(0.0) += scaled;
            }
            Some(Self {
                unit: *unit,
                period_start: date(splits, &["periodStartMs", "period_start_ms"]),
                period_end: date(splits, &["periodEndMs", "period_end_ms"]),
                slices: sorted_slices(&totals),
            })
        })
    }
}

/// The row's source: a string (enum name or numeric string) or a number.
fn source_of(row: &Map<String, Value>) -> GrokbotUsageSource {
    let wire = string(row.get("source"))
        .map(str::to_owned)
        .or_else(|| number(row.get("source")).map(|n| format!("{}", n as i64)));
    GrokbotUsageSource::from_wire(wire.as_deref().unwrap_or(""))
}

/// Sources with usage, most used first; ties keep the source order.
fn sorted_slices(totals: &BTreeMap<GrokbotUsageSource, f64>) -> Vec<BreakdownSlice> {
    let mut slices: Vec<BreakdownSlice> = GrokbotUsageSource::ALL
        .iter()
        .filter_map(|source| {
            totals
                .get(source)
                .filter(|v| v.is_finite() && **v > 0.0)
                .map(|value| BreakdownSlice {
                    source: *source,
                    value: *value,
                })
        })
        .collect();
    // `sort_by` is stable, so equal values keep the source order.
    slices.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    slices
}

/// Epoch milliseconds (int64 arrives as a string in protobuf JSON).
fn date(map: &Map<String, Value>, keys: &[&str]) -> Option<DateTime<Utc>> {
    first_double(map, keys)
        .filter(|ms| *ms > 0.0)
        .and_then(|ms| DateTime::from_timestamp_millis(ms as i64))
}
