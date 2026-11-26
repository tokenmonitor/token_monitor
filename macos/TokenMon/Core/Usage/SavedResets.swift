import Foundation

/// Usage-limit resets the account has saved and can spend when it chooses,
/// as the provider reports them (Codex "usage limit resets", Claude "Reset
/// for free", SuperGrok reset coupons).
///
/// Only the count and the soonest expiry are kept. Redemption identifiers
/// (grant or token ids) are never read, so they never reach a snapshot, a
/// log, or disk.
struct SavedResets: Hashable, Sendable {
    /// Largest count accepted as real; a payload claiming more is treated as
    /// malformed and shows nothing.
    static let maximumCount = 50

    /// Resets available now, between 1 and ``maximumCount``.
    let count: Int
    /// Soonest expiry among the available resets, when the provider reports one.
    let nextExpiry: Date?

    /// A saved-reset inventory, or `nil` when `count` is not between 1 and
    /// ``maximumCount``.
    init?(count: Int, nextExpiry: Date? = nil) {
        guard (1...Self.maximumCount).contains(count) else { return nil }
        self.count = count
        self.nextExpiry = nextExpiry
    }

    /// Builds the inventory from one expiry per reset (`nil` for a reset that
    /// does not expire), leaving out resets already expired at `now`.
    ///
    /// - Returns: `nil` when no reset is left or the count exceeds
    ///   ``maximumCount``.
    init?(expirations: [Date?], now: Date) {
        let live = expirations.filter { expiry in expiry.map { $0 > now } ?? true }
        self.init(count: live.count, nextExpiry: live.compactMap { $0 }.min())
    }
}
