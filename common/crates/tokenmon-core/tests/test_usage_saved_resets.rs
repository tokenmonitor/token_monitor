//! Port of Swift `SavedResetsTests`: the saved-reset inventory shared by the
//! ChatGPT, Claude and SuperGrok panels.

#![allow(clippy::unwrap_used)]

use chrono::{DateTime, Duration, TimeZone, Utc};
use tokenmon_core::usage::SavedResets;

fn now() -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0).unwrap()
}

/// A count outside 1..=50 is rejected; the maximum is accepted.
#[test]
fn test_count_outside_range_is_none() {
    assert!(SavedResets::new(0, None).is_none());
    assert!(SavedResets::new(SavedResets::MAXIMUM_COUNT + 1, None).is_none());
    assert_eq!(
        SavedResets::new(SavedResets::MAXIMUM_COUNT, None)
            .unwrap()
            .count(),
        SavedResets::MAXIMUM_COUNT
    );
}

/// Expired entries (at or before now) are dropped; undated ones count; the
/// soonest remaining expiry wins.
#[test]
fn test_expirations_drop_expired_and_keep_soonest() {
    let soon = now() + Duration::seconds(3_600);
    let later = now() + Duration::seconds(86_400);
    let resets = SavedResets::from_expirations(
        &[
            Some(later),
            Some(now() - Duration::seconds(1)),
            None,
            Some(soon),
            Some(now()),
        ],
        now(),
    )
    .unwrap();
    assert_eq!(resets.count(), 3);
    assert_eq!(resets.next_expiry(), Some(soon));
}

/// Undated resets have no next expiry.
#[test]
fn test_expirations_without_dates_have_no_next_expiry() {
    let resets = SavedResets::from_expirations(&[None, None], now()).unwrap();
    assert_eq!(resets.count(), 2);
    assert_eq!(resets.next_expiry(), None);
}

/// Nothing left (all expired, or empty) yields no inventory.
#[test]
fn test_all_expired_is_none() {
    assert!(SavedResets::from_expirations(&[Some(now() - Duration::seconds(60))], now()).is_none());
    assert!(SavedResets::from_expirations(&[], now()).is_none());
}
