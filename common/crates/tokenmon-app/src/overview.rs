//! Overview data: the saved per-day hourly history, today's live series built
//! from the pollers' hourly data, and the midnight rollover (spec 01 section
//! 4).
//!
//! Portable: the pollers are reached only through the registry's
//! [`HourlySource`](crate::sources::HourlySource) closures, the clock is an
//! argument, and the history file lives in the data directory.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use tokenmon_core::overview::{
    OverviewDaySeries, OverviewHistory, OverviewSeries, ProviderGate, TodayInputs,
    build_today_series, day_for_offset,
};
use tokenmon_core::usage::Calendar;
use tokenmon_core::{Clock, ProviderId};
use tokenmon_ui::panels::overview::{HourActivityInput, HourSeries, OverviewDailyInput};

use crate::registry::ProviderRegistry;
use crate::sessions::Sessions;
use crate::sources::TodayPart;
use tokenmon_core::settings::Settings;

/// The history file inside the data directory (spec 01 section 4.3).
pub const HISTORY_FILE: &str = "activity_overview_hourly_history.dat";

const HOURS: usize = 24;

/// The UI series for a core series.
pub fn hour_series(series: OverviewSeries) -> HourSeries {
    match series {
        OverviewSeries::Grok => HourSeries::Grok,
        OverviewSeries::Grokbot => HourSeries::Grokbot,
        OverviewSeries::Cursor => HourSeries::Cursor,
        OverviewSeries::Claude => HourSeries::Claude,
        OverviewSeries::OpenCodeGo => HourSeries::OpenCodeGo,
        OverviewSeries::OpenCodeZen => HourSeries::OpenCodeZen,
    }
}

/// The 24 hour columns of `series`, each listing the series active in that
/// hour in stack order.
pub fn hour_columns(series: &OverviewDaySeries) -> Vec<Vec<HourActivityInput>> {
    (0..HOURS)
        .map(|hour| {
            OverviewSeries::STACK_ORDER
                .into_iter()
                .filter_map(|s| {
                    let weight = series.weights.get(&s)?.get(hour).copied()?;
                    (weight.is_finite() && weight > 0.0).then(|| HourActivityInput {
                        series: hour_series(s),
                        quota_percent: weight,
                        tokens: series
                            .tokens
                            .get(&s)
                            .and_then(|t| t.get(hour))
                            .map(|t| u64::try_from(*t).unwrap_or(0)),
                    })
                })
                .collect()
        })
        .collect()
}

/// Whether `provider` contributes to the Overview right now: enabled, signed
/// in and not flagged for re-sign-in (OpenCode needs only to be enabled;
/// Grokbot passes Cursor's session gate).
pub fn gate(provider: ProviderId, settings: &Settings, sessions: &Sessions) -> ProviderGate {
    let state = sessions.auth_state(provider);
    ProviderGate {
        enabled: settings.is_provider_enabled(provider),
        signed_in: state.is_signed_in,
        needs_sign_in: state.needs_sign_in,
    }
}

fn contributes(provider: ProviderId, settings: &Settings, sessions: &Sessions) -> bool {
    let g = gate(provider, settings, sessions);
    if provider == ProviderId::OpenCode {
        g.contributes_without_sign_in()
    } else {
        g.contributes()
    }
}

/// Today's series from every contributing provider's hourly data.
pub fn today_series(
    registry: &ProviderRegistry,
    settings: &Settings,
    sessions: &Sessions,
    calendar: &Calendar<'_>,
    today: NaiveDate,
) -> OverviewDaySeries {
    let inputs = ProviderId::USAGE_PROVIDERS
        .into_iter()
        .filter(|p| contributes(*p, settings, sessions))
        .filter_map(|p| registry.hourly(p).and_then(|source| source(calendar)))
        .fold(TodayInputs::default(), |mut acc, part| {
            match part {
                TodayPart::Grok(d) => acc.grok = Some(d),
                TodayPart::Claude(d) => acc.claude = Some(d),
                TodayPart::Grokbot(d) => acc.grokbot = Some(d),
                TodayPart::Cursor(d) => acc.cursor = Some(d),
                TodayPart::OpenCode(d) => acc.open_code = Some(d),
            }
            acc
        });
    build_today_series(&inputs, today)
}

/// Records today's live series into `store` so history accumulates while the
/// panel is closed. Returns whether anything changed.
pub fn record_live(
    store: &OverviewStore,
    registry: &ProviderRegistry,
    settings: &Settings,
    sessions: &Sessions,
    now: DateTime<Utc>,
    clock: &dyn Clock,
) -> bool {
    let calendar = Calendar::local(clock);
    let today = calendar.date_of(now);
    let live = today_series(registry, settings, sessions, &calendar, today);
    store.record(&live, today)
}

/// The first local midnight after `now`: when the Overview must roll to a new
/// day.
///
/// Computed from the next calendar date's own midnight, so a 23 or 25 hour
/// day (DST change) still rolls over at local midnight.
pub fn next_midnight(now: DateTime<Utc>, calendar: &Calendar<'_>) -> DateTime<Utc> {
    calendar
        .date_of(now)
        .succ_opt()
        .and_then(|d| calendar.start_of_date(d))
        .filter(|m| *m > now)
        .unwrap_or_else(|| now + Duration::days(1))
}

/// The persisted Overview history.
pub struct OverviewStore {
    path: PathBuf,
    history: Mutex<OverviewHistory>,
}

impl OverviewStore {
    /// Loads the history from `directory` (empty when missing or damaged).
    pub fn open(directory: &Path) -> Self {
        let path = directory.join(HISTORY_FILE);
        Self {
            history: Mutex::new(OverviewHistory::load(&path)),
            path,
        }
    }

    fn history(&self) -> std::sync::MutexGuard<'_, OverviewHistory> {
        self.history.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Merges `series` into `date` and saves when anything changed. Returns
    /// whether it did.
    pub fn record(&self, series: &OverviewDaySeries, date: NaiveDate) -> bool {
        let mut history = self.history();
        let changed = history.record(series, date);
        if changed && let Err(e) = history.save(&self.path) {
            tracing::warn!(error = %e, "overview history not saved");
        }
        changed
    }

    /// The saved series for `date`.
    pub fn series(&self, date: NaiveDate) -> Option<OverviewDaySeries> {
        self.history().series(date).cloned()
    }

    /// The earliest saved day.
    pub fn earliest_day(&self) -> Option<NaiveDate> {
        self.history().earliest_day()
    }

    /// The chart input for `day_offset` (`0` today, negative earlier): today
    /// is recorded first so the chart includes the newest hour.
    pub fn daily_input(
        &self,
        day_offset: i32,
        live: &OverviewDaySeries,
        today: NaiveDate,
    ) -> OverviewDailyInput {
        self.record(live, today);
        let date = day_for_offset(today, day_offset);
        let hours = self
            .series(date)
            .map_or_else(|| vec![Vec::new(); HOURS], |s| hour_columns(&s));
        OverviewDailyInput {
            day_offset,
            hours,
            earliest_day: self.earliest_day(),
        }
    }
}
