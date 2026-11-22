import Foundation

/// One Claude rate-limit window (`five_hour` / `seven_day`).
struct ClaudeUsageWindow: Hashable, Sendable {
    /// 0…100 utilization of the window.
    var usedPercent: Double
    var resetsAt: Date?
    /// Model the limit applies to (`"Fable"`) when it is model-scoped;
    /// `nil` for an account-wide limit.
    var scopeName: String?

    var remainingPercent: Double {
        Percent.clamp(100 - usedPercent)
    }
}

/// Parsed `claude.ai/api/organizations/{org}/usage` payload.
struct ClaudeUsageResponse: Hashable, Sendable {
    var fiveHour: ClaudeUsageWindow?
    var sevenDay: ClaudeUsageWindow?
    /// Saved "Reset for free" resets, from the `cedar_ember` block.
    var savedResets: SavedResets?

    /// Parses the raw usage JSON.
    ///
    /// The top-level `five_hour` / `seven_day` objects win when present. Accounts
    /// whose `seven_day` is `null` report their weekly pool only in the `limits`
    /// array (`group: "weekly"`), and that entry is used instead. `now` decides
    /// which saved-reset grants are currently available.
    static func parse(_ data: Data, now: Date = Date()) throws -> ClaudeUsageResponse {
        let root = try ProviderHTTP.jsonObject(data, context: .claude)
        let limits = (root["limits"] as? [[String: Any]]) ?? []
        return ClaudeUsageResponse(
            fiveHour: window(from: root["five_hour"]) ?? limitWindow(in: limits, group: "session"),
            sevenDay: window(from: root["seven_day"]) ?? limitWindow(in: limits, group: "weekly"),
            savedResets: ClaudeResetGrants.savedResets(from: root["cedar_ember"], now: now)
        )
    }

    private static func window(from raw: Any?) -> ClaudeUsageWindow? {
        guard let dict = raw as? [String: Any],
              let utilization = JSON.number(dict["utilization"])
        else { return nil }
        return ClaudeUsageWindow(usedPercent: Percent.clamp(utilization), resetsAt: resetDate(dict["resets_at"]))
    }

    /// Picks the `limits` entry for `group`: an account-wide (unscoped) limit
    /// first, then an active one, then the most used.
    private static func limitWindow(in limits: [[String: Any]], group: String) -> ClaudeUsageWindow? {
        let candidates = limits.filter { JSON.string($0["group"]) == group && JSON.number($0["percent"]) != nil }
        func rank(_ entry: [String: Any]) -> (Int, Int, Double) {
            let unscoped = scopeName(entry["scope"]) == nil ? 1 : 0
            let active = (entry["is_active"] as? Bool) == true ? 1 : 0
            return (unscoped, active, JSON.number(entry["percent"]) ?? 0)
        }
        guard let best = candidates.max(by: { rank($0) < rank($1) }),
              let percent = JSON.number(best["percent"])
        else { return nil }
        return ClaudeUsageWindow(
            usedPercent: Percent.clamp(percent),
            resetsAt: resetDate(best["resets_at"]),
            scopeName: scopeName(best["scope"])
        )
    }

    /// Display name of a limit's model scope, `nil` when the limit is unscoped.
    private static func scopeName(_ raw: Any?) -> String? {
        guard let scope = raw as? [String: Any],
              let model = scope["model"] as? [String: Any]
        else { return nil }
        return JSON.string(model["display_name"]) ?? JSON.string(model["id"])
    }

    private static func resetDate(_ raw: Any?) -> Date? {
        (raw as? String).flatMap(ISO8601DateFormatter.parseFlexible)
    }
}

/// Saved-reset grants in the usage payload's `cedar_ember` block:
/// `{"eligible": true, "grants": [{"resets_left": 1, "resets_total": 1,
/// "paused": false, "starts_at": "…", "ends_at": "…"}]}`.
///
/// Grant ids are not read. A grant with an unreadable count, pause state or
/// bound is dropped without hiding the others.
enum ClaudeResetGrants {
    /// Resets available at `now`, or `nil` when the block is absent, the
    /// account is not eligible, or no grant offers a reset.
    static func savedResets(from raw: Any?, now: Date) -> SavedResets? {
        guard let block = raw as? [String: Any],
              block["eligible"] as? Bool == true,
              let grants = block["grants"] as? [[String: Any]]
        else { return nil }
        var expirations: [Date?] = []
        for grant in grants {
            guard let available = availableResets(in: grant, now: now) else { continue }
            guard available.count <= SavedResets.maximumCount - expirations.count else { return nil }
            expirations += repeatElement(available.expiry, count: available.count)
        }
        return SavedResets(expirations: expirations, now: now)
    }

    /// Resets one grant offers at `now` and their expiry; `nil` for a
    /// malformed grant. A paused, used-up, not-yet-started or expired grant
    /// offers zero.
    private static func availableResets(in grant: [String: Any], now: Date) -> (count: Int, expiry: Date?)? {
        guard let left = JSON.number(grant["resets_left"]).flatMap({ Int(exactly: $0) }), left >= 0,
              let paused = grant["paused"] as? Bool,
              let startsAt = bound(grant["starts_at"]),
              let endsAt = bound(grant["ends_at"])
        else { return nil }
        if let total = JSON.number(grant["resets_total"]), Double(left) > total { return nil }
        if paused || startsAt.map({ $0 > now }) == true || endsAt.map({ $0 <= now }) == true {
            return (0, nil)
        }
        return (left, endsAt)
    }

    /// An absent or null bound is open (`.some(nil)`); an unreadable one is
    /// malformed (`nil`).
    private static func bound(_ raw: Any?) -> Date?? {
        guard let raw, !(raw is NSNull) else { return .some(nil) }
        guard let string = raw as? String, let date = ISO8601DateFormatter.parseFlexible(string) else { return nil }
        return .some(date)
    }
}

/// Headline Claude rate-limit snapshot (5-hour and weekly windows).
struct ClaudeSnapshot: Identifiable, Hashable, Sendable {
    var id: Date { fetchedAt }
    var fetchedAt: Date
    var fiveHour: ClaudeUsageWindow?
    var sevenDay: ClaudeUsageWindow?
    var accountEmail: String?
    var savedResets: SavedResets?

    var resetsAt: Date? { fiveHour?.resetsAt ?? sevenDay?.resetsAt }

    /// Weekly track label, naming the model when the weekly limit is model-scoped.
    var weeklyLabel: String {
        sevenDay?.scopeName.map { "Weekly · \($0)" } ?? "Weekly"
    }

    var headlineUsedPercent: Double {
        (fiveHour ?? sevenDay)?.usedPercent ?? 0
    }
}

extension ClaudeSnapshot {
    /// Five-hour session + weekly pool.
    var usagePool: UsagePool {
        .combining([
            fiveHour != nil ? .hourly : nil,
            sevenDay != nil ? .weekly : nil
        ].compactMap { $0 })
    }
}
