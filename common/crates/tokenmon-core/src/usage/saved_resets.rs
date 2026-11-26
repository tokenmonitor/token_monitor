//! Usage-limit resets an account has saved and can spend when it chooses.

use chrono::{DateTime, Utc};

/// Usage-limit resets the account has saved and can spend when it chooses, as
/// the provider reports them (Codex "usage limit resets", Claude "Reset for
/// free", SuperGrok reset coupons).
///
/// Only the count and the soonest expiry are kept. Redemption identifiers
/// (grant or token ids) are never read, so they never reach a snapshot, a log
/// or disk.
///
/// # Examples
///
/// ```
/// use tokenmon_core::usage::SavedResets;
/// assert_eq!(SavedResets::new(3, None).unwrap().count(), 3);
/// assert!(SavedResets::new(0, None).is_none());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SavedResets {
    count: u32,
    next_expiry: Option<DateTime<Utc>>,
}

impl SavedResets {
    /// Largest count accepted as real; a payload claiming more is treated as
    /// malformed and shows nothing.
    pub const MAXIMUM_COUNT: u32 = 50;

    /// A saved-reset inventory, or `None` when `count` is not between 1 and
    /// [`Self::MAXIMUM_COUNT`].
    #[must_use]
    pub fn new(count: u32, next_expiry: Option<DateTime<Utc>>) -> Option<Self> {
        (1..=Self::MAXIMUM_COUNT)
            .contains(&count)
            .then_some(Self { count, next_expiry })
    }

    /// Builds the inventory from one expiry per reset (`None` for a reset that
    /// does not expire), leaving out resets already expired at `now`.
    ///
    /// Returns `None` when no reset is left or the count exceeds
    /// [`Self::MAXIMUM_COUNT`].
    #[must_use]
    pub fn from_expirations(
        expirations: &[Option<DateTime<Utc>>],
        now: DateTime<Utc>,
    ) -> Option<Self> {
        let live = || expirations.iter().filter(|e| e.is_none_or(|at| at > now));
        let count = u32::try_from(live().count()).ok()?;
        Self::new(count, live().filter_map(|e| *e).min())
    }

    /// Resets available now, between 1 and [`Self::MAXIMUM_COUNT`].
    #[must_use]
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Soonest expiry among the available resets, when the provider reports one.
    #[must_use]
    pub fn next_expiry(&self) -> Option<DateTime<Utc>> {
        self.next_expiry
    }
}
