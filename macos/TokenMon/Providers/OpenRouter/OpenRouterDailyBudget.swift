import Foundation

/// Daily-budget bars for the OpenRouter panel, measured against the same pool
/// as the credit bar.
///
/// A weekly or monthly key limit uses that reset window. A daily key limit
/// shows seven UTC days, each against that day's own cap. Account credits and
/// a key cap that never resets have no billing window, so the bars are the
/// UTC week and each day's height is its share of the whole balance.
struct OpenRouterDailyChart {
    /// Which shared bar chart draws these days.
    enum Kind: Equatable, Sendable {
        case weekly
        case monthly
        /// Seven independent daily caps. Each stem is that day's own limit.
        case dailyCap
    }

    var kind: Kind
    /// Absolute reset instant for a weekly or monthly key limit.
    var resetsAt: Date?
    /// Start of a monthly key limit, for the pace footer.
    var periodStart: Date?
    /// Live used percent when it measures the same pool the bars pace.
    var periodUsedPercent: Double?
    private var budgetUSD: Double
    private var spendByUTCDay: [String: Double]
    private var now: Date
    private var calendar: Calendar

    /// Nil when there is no budget to divide a day's spend by.
    static func make(
        for snapshot: OpenRouterSnapshot,
        now: Date = Date(),
        calendar: Calendar = .current
    ) -> OpenRouterDailyChart? {
        guard let budgetUSD = snapshot.budgetUSD, budgetUSD > 0 else { return nil }
        let utc = Self.utcCalendar
        let reset = snapshot.limitReset
        let kind: Kind
        let resetsAt: Date?
        let periodStart: Date?
        let periodUsedPercent: Double?
        switch reset {
        case "daily":
            kind = .dailyCap
            resetsAt = nil
            periodStart = nil
            periodUsedPercent = nil
        case "monthly":
            kind = .monthly
            resetsAt = utc.dateInterval(of: .month, for: now)?.end
            periodStart = utc.dateInterval(of: .month, for: now)?.start
            periodUsedPercent = snapshot.usedPercent
        case "weekly":
            kind = .weekly
            resetsAt = utc.dateInterval(of: .weekOfYear, for: now)?.end
            periodStart = nil
            periodUsedPercent = snapshot.usedPercent
        default:
            kind = .weekly
            resetsAt = nil
            periodStart = nil
            periodUsedPercent = nil
        }
        return OpenRouterDailyChart(
            kind: kind,
            resetsAt: resetsAt,
            periodStart: periodStart,
            periodUsedPercent: periodUsedPercent,
            budgetUSD: budgetUSD,
            spendByUTCDay: snapshot.spendByUTCDay,
            now: now,
            calendar: calendar
        )
    }

    /// Bars for the window `weekOffset` steps before the one containing `now`.
    /// Dates are the UTC civil day shown on `calendar`, so the label reads the
    /// UTC date OpenRouter reported.
    func days(weekOffset: Int) -> [DailyBudgetDay] {
        let utc = Self.utcCalendar
        switch kind {
        case .dailyCap:
            return dailyCapDays(weekOffset: weekOffset, utc: utc)
        case .weekly:
            guard let end = resetsAt ?? utc.dateInterval(of: .weekOfYear, for: now)?.end else { return [] }
            return windowDays(
                DailyBudget.buildWeeklyWindowDays(
                    limitUSD: 100,
                    daysInPeriod: 7,
                    resetsAt: end,
                    spentByDay: percentByDay(utc: utc),
                    weekOffset: weekOffset,
                    now: now,
                    calendar: utc
                ),
                utc: utc
            )
        case .monthly:
            guard let built = DailyBudget.buildSubscriptionMonthLast7Days(
                limitUSD: 100,
                spentByDay: percentByDay(utc: utc),
                resetsAt: resetsAt,
                weekOffset: weekOffset,
                now: now,
                calendar: utc
            ) else { return [] }
            return windowDays(built.days, utc: utc)
        }
    }

    private func dailyCapDays(weekOffset: Int, utc: Calendar) -> [DailyBudgetDay] {
        let today = utc.startOfDay(for: now)
        return (0 ..< 7).compactMap { offset in
            guard let day = utc.date(byAdding: .day, value: weekOffset * 7 - 6 + offset, to: today) else {
                return nil
            }
            return DailyBudgetDay(
                date: displayed(day, utc: utc),
                spentUSD: percent(on: day, utc: utc),
                budgetUSD: 100
            )
        }
    }

    private func windowDays(_ days: [DailyBudgetDay], utc: Calendar) -> [DailyBudgetDay] {
        days.map { day in
            DailyBudgetDay(
                date: displayed(day.date, utc: utc),
                spentUSD: day.spentUSD,
                budgetUSD: day.budgetUSD,
                isPriorWindow: day.isPriorWindow,
                tokens: day.tokens
            )
        }
    }

    /// Percent of `budgetUSD` spent on the UTC day of `date`.
    private func percent(on date: Date, utc: Calendar) -> Double {
        let parts = utc.dateComponents([.year, .month, .day], from: date)
        guard let year = parts.year, let month = parts.month, let day = parts.day else { return 0 }
        let key = String(format: "%04d-%02d-%02d", year, month, day)
        return max(0, (spendByUTCDay[key] ?? 0) / budgetUSD * 100)
    }

    private func percentByDay(utc: Calendar) -> [Date: Double] {
        var spent: [Date: Double] = [:]
        for (key, usd) in spendByUTCDay {
            guard let date = Self.date(utcDay: key, calendar: utc) else { continue }
            spent[utc.startOfDay(for: date)] = max(0, usd / budgetUSD * 100)
        }
        return spent
    }

    /// `yyyy-MM-dd` as a start of day on `calendar`.
    private static func date(utcDay: String, calendar: Calendar) -> Date? {
        let parts = utcDay.split(separator: "-")
        guard parts.count == 3, let year = Int(parts[0]), let month = Int(parts[1]), let day = Int(parts[2]) else {
            return nil
        }
        return calendar.date(from: DateComponents(year: year, month: month, day: day))
    }

    /// The UTC calendar day, as midnight on the chart's calendar.
    private func displayed(_ date: Date, utc: Calendar) -> Date {
        let parts = utc.dateComponents([.year, .month, .day], from: date)
        return calendar.date(from: DateComponents(year: parts.year, month: parts.month, day: parts.day)) ?? date
    }

    private static let utcCalendar: Calendar = {
        var calendar = Calendar(identifier: .iso8601)
        calendar.timeZone = TimeZone(secondsFromGMT: 0) ?? calendar.timeZone
        return calendar
    }()
}
