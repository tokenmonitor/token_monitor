//! Persistence failure handling, same-day de-duplication rules, optional
//! export fields and date-conversion bounds of the Grok crate.
#![allow(clippy::unwrap_used, clippy::assert_is_empty)]

mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::{clock_at, utc};
use serde_json::Value;
use tokenmon_provider_grok::export::{ExportFormat, export};
use tokenmon_provider_grok::history::{BackingRead, FileBacking, GrokHistoryStore, HistoryBacking};
use tokenmon_provider_grok::models::{
    ProductUsage, WeeklyUsageSnapshot, apple_date, synthesize_products,
};

/// Backing whose writes can be made to fail and are counted.
struct Flaky {
    fail: Arc<AtomicBool>,
    writes: Arc<AtomicUsize>,
    saved: Arc<Mutex<Option<String>>>,
    read: fn() -> BackingRead,
}

impl HistoryBacking for Flaky {
    fn read(&self) -> BackingRead {
        (self.read)()
    }
    fn write(&self, text: &str) -> bool {
        self.writes.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return false;
        }
        *self.saved.lock().unwrap() = Some(text.to_owned());
        true
    }
}

struct Rig {
    store: GrokHistoryStore,
    fail: Arc<AtomicBool>,
    writes: Arc<AtomicUsize>,
    saved: Arc<Mutex<Option<String>>>,
}

fn rig(read: fn() -> BackingRead) -> Rig {
    let fail = Arc::new(AtomicBool::new(false));
    let writes = Arc::new(AtomicUsize::new(0));
    let saved = Arc::new(Mutex::new(None));
    let backing = Flaky {
        fail: fail.clone(),
        writes: writes.clone(),
        saved: saved.clone(),
        read,
    };
    let store = GrokHistoryStore::open(Some(Box::new(backing)), clock_at(utc(2026, 7, 15, 12, 0)));
    Rig {
        store,
        fail,
        writes,
        saved,
    }
}

fn snap(hour: u32, used: f64) -> WeeklyUsageSnapshot {
    WeeklyUsageSnapshot::new(utc(2026, 7, 15, hour, 0), used)
}

/// A failed write keeps the store dirty so the next flush retries and
/// succeeds; a clean store does not write at all.
#[test]
fn test_failed_flush_stays_dirty_and_retries() {
    let mut r = rig(|| BackingRead::Missing);
    r.fail.store(true, Ordering::SeqCst);
    r.store.append(snap(9, 10.0));
    r.store.flush();
    assert!(r.store.is_dirty());
    assert!(r.saved.lock().unwrap().is_none());
    r.fail.store(false, Ordering::SeqCst);
    r.store.flush();
    assert!(!r.store.is_dirty());
    assert!(r.saved.lock().unwrap().is_some());
    let writes = r.writes.load(Ordering::SeqCst);
    r.store.flush();
    assert_eq!(
        r.writes.load(Ordering::SeqCst),
        writes,
        "clean flush is free"
    );
}

/// A history that could not be read is never written over.
#[test]
fn test_unreadable_history_is_never_overwritten() {
    let mut r = rig(|| BackingRead::Unreadable);
    r.store.append(snap(9, 10.0));
    r.store.flush();
    assert!(r.store.store_failed());
    assert_eq!(r.writes.load(Ordering::SeqCst), 0);
    assert_eq!(r.store.all_snapshots().len(), 1, "kept in memory");
}

/// The file backing creates missing parent directories on write.
#[test]
fn test_file_backing_creates_parent_directories() {
    let dir = tempfile::tempdir().unwrap();
    let backing = FileBacking::in_dir(&dir.path().join("a").join("b"));
    assert!(backing.write("{}"));
    assert!(backing.path().exists());
    assert!(matches!(backing.read(), BackingRead::Text(t) if t == "{}"));
}

/// The file backing reports failure when the parent is a regular file.
#[test]
fn test_file_backing_write_fails_under_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    assert!(!FileBacking::in_dir(&blocker).write("{}"));
}

/// An idle same-day poll with identical usage leaves the day's row alone, but
/// any real difference (remaining, reset, products, account) replaces it.
#[test]
fn test_same_day_row_is_replaced_only_by_real_changes() {
    let reset = Some(utc(2026, 7, 20, 0, 0));
    let base = || snap(9, 10.0).with_resets_at(reset);
    let mut store = GrokHistoryStore::in_memory(clock_at(utc(2026, 7, 15, 12, 0)));
    store.append(base());
    let mut later = base();
    later.fetched_at = utc(2026, 7, 15, 11, 0);
    store.append(later);
    assert_eq!(store.all_snapshots()[0].fetched_at, utc(2026, 7, 15, 9, 0));

    let variants = [
        base().with_remaining(50.0),
        base().with_resets_at(Some(utc(2026, 7, 21, 0, 0))),
        base().with_products(vec![ProductUsage::new("chat", "Chat", 10.0)]),
    ];
    for (i, mut changed) in variants.into_iter().enumerate() {
        changed.fetched_at = utc(2026, 7, 15, 10 + u32::try_from(i).unwrap(), 0);
        let at = changed.fetched_at;
        store.append(changed);
        assert_eq!(store.all_snapshots().len(), 1);
        assert_eq!(store.all_snapshots()[0].fetched_at, at, "variant {i}");
    }
}

/// Exports omit absent reset and account fields and include present ones.
#[test]
fn test_export_omits_missing_optional_fields() {
    let bare = snap(9, 10.0);
    let full = snap(10, 20.0)
        .with_resets_at(Some(utc(2026, 7, 20, 0, 0)))
        .with_account(Some("me@x.co".to_owned()));
    let json = export(&[bare, full], ExportFormat::Json).unwrap();
    let rows: Vec<Value> = serde_json::from_slice(&json).unwrap();
    assert!(rows[0].get("resetsAt").is_none());
    assert!(rows[0].get("accountEmail").is_none());
    assert_eq!(rows[1]["resetsAt"], "2026-07-20T00:00:00Z");
    assert_eq!(rows[1]["accountEmail"], "me@x.co");
}

/// Non-finite and out-of-range Apple timestamps are rejected.
#[test]
fn test_from_apple_rejects_non_finite_and_huge_values() {
    assert_eq!(apple_date::from_apple(f64::NAN), None);
    assert_eq!(apple_date::from_apple(f64::INFINITY), None);
    assert_eq!(apple_date::from_apple(1e300), None);
    let at = apple_date::from_apple(0.0).unwrap();
    assert_eq!(at, utc(2001, 1, 1, 0, 0));
}

/// Usage at or below 0.05% has no synthesized product, just above it does.
#[test]
fn test_synthesize_products_threshold_is_exclusive() {
    assert!(synthesize_products(0.05).is_empty());
    assert_eq!(synthesize_products(0.06).len(), 1);
}
