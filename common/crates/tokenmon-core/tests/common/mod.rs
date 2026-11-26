//! Shared helpers for the storage tests.
#![allow(dead_code, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
use tokenmon_core::ManualClock;
use tokenmon_core::storage::TextStore;

/// A text store whose writes fail while `fail_writes` is set, like a full disk
/// or a read-only directory. Reads see only what a successful write stored.
#[derive(Default)]
pub struct FlakyStore {
    values: Mutex<BTreeMap<String, String>>,
    pub fail_writes: AtomicBool,
}

impl FlakyStore {
    pub fn failing() -> Arc<Self> {
        let store = Self::default();
        store.fail_writes.store(true, Ordering::SeqCst);
        Arc::new(store)
    }

    pub fn recover(&self) {
        self.fail_writes.store(false, Ordering::SeqCst);
    }
}

impl TextStore for FlakyStore {
    fn value(&self, key: &str) -> Option<String> {
        self.values.lock().unwrap().get(key).cloned()
    }

    fn set(&self, value: &str, key: &str) -> bool {
        if self.fail_writes.load(Ordering::SeqCst) {
            return false;
        }
        self.values
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        true
    }

    fn remove(&self, key: &str) {
        self.values.lock().unwrap().remove(key);
    }
}

/// 2026-10-10 00:00 UTC, the "today" of every storage test.
pub fn today_midnight() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 10, 0, 0, 0).unwrap()
}

/// An instant `days` calendar days and `hour` hours after today's UTC midnight.
pub fn at(days: i64, hour: i64) -> DateTime<Utc> {
    today_midnight() + Duration::days(days) + Duration::hours(hour)
}

/// A UTC clock positioned at `now`.
pub fn utc_clock(now: DateTime<Utc>) -> Arc<ManualClock> {
    Arc::new(ManualClock::new(now))
}

/// A clock positioned at `now` with a fixed local offset of `hours` east of UTC.
pub fn offset_clock(now: DateTime<Utc>, hours: i32) -> Arc<ManualClock> {
    Arc::new(ManualClock::new(now).with_offset(FixedOffset::east_opt(hours * 3600).unwrap()))
}
