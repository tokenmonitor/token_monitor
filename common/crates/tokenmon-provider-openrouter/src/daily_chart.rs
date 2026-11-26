//! Daily-budget bars for the OpenRouter panel, measured against the same pool
//! as the credit bar (`OpenRouterDailyChart`).
//!
//! A weekly or monthly key limit uses that reset window. A daily key limit
//! shows seven UTC days, each against that day's own cap. Account credits and
//! a key cap that never resets have no billing window, so the bars are the
//! UTC week and each day's height is its share of the whole balance.

use chrono::{DateTime, Datelike, Days, Months, NaiveDate, Utc};
use tokenmon_core::usage::budget::SpentByDay;
use tokenmon_core::usage::{Calendar, DailyBudget, DailyBudgetDay, WeekOffset};

use crate::snapshot::{BudgetSource, LimitReset, OpenRouterSnapshot};

/// Which shared bar chart draws the days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    /// A Monday-to-Sunday window of a weekly pool.
    Weekly,
    /// The last seven days of a monthly pool.
    Monthly,
    /// Seven independent daily caps; each stem is that day's own limit.
    DailyCap,
}

/// The bars of one OpenRouter snapshot and what paces them.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenRouterDailyChart {
    /// Which shared chart draws these days.
    pub kind: ChartKind,
    /// Absolute reset instant for a weekly or monthly key limit.
    pub resets_at: Option<DateTime<Utc>>,
    /// Start of a monthly key limit, for the pace footer.
    pub period_start: Option<DateTime<Utc>>,
    /// Live used percent when it measures the same pool the bars pace.
    pub period_used_percent: Option<f64>,
    budget_usd: f64,
    spend_by_utc_day: std::collections::BTreeMap<NaiveDate, f64>,
    now: DateTime<Utc>,
}

/// Midnight UTC of the first day of the month containing `day`, and of the next.
fn month_bounds(day: NaiveDate) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let start = day.with_day(1)?;
    let end = start.checked_add_months(Months::new(1))?;
    Some((
        start.and_hms_opt(0, 0, 0)?.and_utc(),
        end.and_hms_opt(0, 0, 0)?.and_utc(),
    ))
}

/// Midnight UTC of the Monday after `day` (the end of its Monday-Sunday week).
fn week_end(day: NaiveDate) -> Option<DateTime<Utc>> {
    let next = day.checked_add_days(Days::new(u64::from(
        7 - day.weekday().num_days_from_monday(),
    )))?;
    Some(next.and_hms_opt(0, 0, 0)?.and_utc())
}

impl OpenRouterDailyChart {
    /// The chart for `snapshot` at `now`; `None` when there is no budget to
    /// divide a day's spend by.
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::{TimeZone, Utc};
    /// use tokenmon_provider_openrouter::{
    ///     NoCostEstimator, OpenRouterSnapshot, daily_chart::OpenRouterDailyChart, parse_key,
    /// };
    /// let now = Utc.with_ymd_and_hms(2026, 1, 31, 23, 30, 0).unwrap();
    /// let key = parse_key(br#"{"data":{"usage":20}}"#).unwrap();
    /// let snap = OpenRouterSnapshot::build(&key, None, None, now, &NoCostEstimator);
    /// assert!(OpenRouterDailyChart::make(&snap, now).is_none());
    /// ```
    #[must_use]
    pub fn make(snapshot: &OpenRouterSnapshot, now: DateTime<Utc>) -> Option<Self> {
        let budget_usd = snapshot.budget_usd.filter(|b| *b > 0.0)?;
        let today = now.date_naive();
        let reset = if snapshot.budget_source == Some(BudgetSource::KeyLimit) {
            snapshot.limit_reset
        } else {
            None
        };
        let (kind, resets_at, period_start, period_used_percent) = match reset {
            Some(LimitReset::Daily) => (ChartKind::DailyCap, None, None, None),
            Some(LimitReset::Monthly) => {
                let bounds = month_bounds(today);
                (
                    ChartKind::Monthly,
                    bounds.map(|b| b.1),
                    bounds.map(|b| b.0),
                    snapshot.used_percent(),
                )
            }
            Some(LimitReset::Weekly) => (
                ChartKind::Weekly,
                week_end(today),
                None,
                snapshot.used_percent(),
            ),
            Some(LimitReset::Other) | None => (ChartKind::Weekly, None, None, None),
        };
        Some(Self {
            kind,
            resets_at,
            period_start,
            period_used_percent,
            budget_usd,
            spend_by_utc_day: snapshot.spend_by_utc_day.clone(),
            now,
        })
    }

    /// Bars for the window `week_offset` steps before the one containing `now`.
    /// Dates are the UTC civil day shown at midnight on `display`, so the label
    /// reads the UTC date OpenRouter reported.
    #[must_use]
    pub fn days(&self, week_offset: WeekOffset, display: &Calendar<'_>) -> Vec<DailyBudgetDay> {
        let utc = Calendar::utc();
        let spent = self.percent_by_day(&utc);
        let built = match self.kind {
            ChartKind::DailyCap => return self.daily_cap_days(week_offset, &utc, display),
            ChartKind::Weekly => {
                let end = self.resets_at.or_else(|| week_end(self.now.date_naive()));
                let Some(end) = end else { return Vec::new() };
                DailyBudget::build_weekly_window_days(
                    100.0,
                    7,
                    end,
                    &spent,
                    None,
                    None,
                    week_offset,
                    self.now,
                    &utc,
                )
            }
            ChartKind::Monthly => {
                let Some((days, _)) = DailyBudget::build_subscription_month_last_7_days(
                    100.0,
                    &spent,
                    None,
                    self.resets_at,
                    &SpentByDay::new(),
                    week_offset,
                    self.now,
                    &utc,
                ) else {
                    return Vec::new();
                };
                days
            }
        };
        built
            .into_iter()
            .map(|d| DailyBudgetDay {
                date: displayed(d.date, &utc, display),
                ..d
            })
            .collect()
    }

    fn daily_cap_days(
        &self,
        week_offset: WeekOffset,
        utc: &Calendar<'_>,
        display: &Calendar<'_>,
    ) -> Vec<DailyBudgetDay> {
        let today = utc.start_of_day(self.now);
        let first = i64::from(week_offset.get()) * 7 - 6;
        (0..7)
            .filter_map(|o| {
                let day = utc.add_days(today, first + o)?;
                Some(DailyBudgetDay::new(
                    displayed(day, utc, display),
                    self.percent_on(utc.date_of(day)),
                    100.0,
                ))
            })
            .collect()
    }

    /// Percent of the budget spent on a UTC day.
    fn percent_on(&self, day: NaiveDate) -> f64 {
        (self.spend_by_utc_day.get(&day).copied().unwrap_or(0.0) / self.budget_usd * 100.0).max(0.0)
    }

    fn percent_by_day(&self, utc: &Calendar<'_>) -> SpentByDay {
        self.spend_by_utc_day
            .keys()
            .filter_map(|day| Some((utc.start_of_date(*day)?, self.percent_on(*day))))
            .collect()
    }
}

/// The UTC calendar day of `date`, as midnight on `display`.
fn displayed(date: DateTime<Utc>, utc: &Calendar<'_>, display: &Calendar<'_>) -> DateTime<Utc> {
    display.start_of_date(utc.date_of(date)).unwrap_or(date)
}
