@testable import TokenMon
import XCTest

final class OpenRouterDailyBudgetTests: XCTestCase {
    private let utc: Calendar = {
        var calendar = Calendar(identifier: .iso8601)
        calendar.timeZone = TimeZone(secondsFromGMT: 0) ?? calendar.timeZone
        return calendar
    }()

    /// Saturday 31 Jan 2026, 23:30 UTC. Inside the Monday–Sunday week that ends 2 Feb.
    private var now: Date {
        guard let date = utc.date(from: DateComponents(year: 2026, month: 1, day: 31, hour: 23, minute: 30)) else {
            preconditionFailure("fixture clock")
        }
        return date
    }

    /// A weekly key limit keeps each UTC day's spend as a percent of that limit.
    /// Saturday 31 Jan 2026 is inside the Monday–Sunday week that resets 2 Feb.
    func testWeeklyKeyLimitUsesThatWeeksSpend() throws {
        var key = Self.key(limit: 70, reset: "weekly")
        key.usageDaily = nil
        let activity = [row(date: "2026-01-31", usage: 10), row(date: "2026-01-31T00:00:00Z", usage: 4)]
        let snapshot = OpenRouterSnapshot.build(key: key, credits: nil, activity: activity, fetchedAt: now)
        XCTAssertEqual(snapshot.limitReset, "weekly")
        XCTAssertEqual(snapshot.spendByUTCDay["2026-01-31"] ?? -1, 14, accuracy: 0.001)

        let chart = try XCTUnwrap(OpenRouterDailyChart.make(for: snapshot, now: now, calendar: utc))
        XCTAssertEqual(chart.kind, .weekly)
        XCTAssertEqual(chart.periodUsedPercent ?? -1, snapshot.usedPercent ?? -1, accuracy: 0.001)
        let days = chart.days(weekOffset: 0)
        XCTAssertEqual(days.count, 7)
        let saturday = try XCTUnwrap(days.first { utc.component(.day, from: $0.date) == 31 })
        XCTAssertEqual(saturday.spentUSD, 14 / 70 * 100, accuracy: 0.001)
        XCTAssertEqual(saturday.budgetUSD, 100.0 / 7, accuracy: 0.001)
        XCTAssertEqual(utc.component(.day, from: days[0].date), 26)
        XCTAssertEqual(chart.resetsAt, utc.date(from: DateComponents(year: 2026, month: 2, day: 2)))
    }

    /// A daily cap compares each UTC day with that day's own limit, not a weekly share.
    func testDailyCapIsEachDaysOwnLimit() throws {
        var key = Self.key(limit: 10, reset: "daily")
        key.usageDaily = 5
        let snapshot = OpenRouterSnapshot.build(key: key, credits: nil, activity: nil, fetchedAt: now)
        let chart = try XCTUnwrap(OpenRouterDailyChart.make(for: snapshot, now: now, calendar: utc))
        XCTAssertEqual(chart.kind, .dailyCap)
        XCTAssertNil(chart.periodUsedPercent)
        let days = chart.days(weekOffset: 0)
        XCTAssertEqual(days.count, 7)
        let today = try XCTUnwrap(days.last)
        XCTAssertEqual(utc.component(.day, from: today.date), 31)
        XCTAssertEqual(today.spentUSD, 50, accuracy: 0.001)
        XCTAssertEqual(today.budgetUSD, 100, accuracy: 0.001)
        XCTAssertEqual(days.dropLast().map(\.spentUSD), Array(repeating: 0, count: 6))
    }

    /// Account credits keep the credit pool. The key's own reset is not the chart's window,
    /// and today's `/key` spend fills a day activity did not report.
    func testAccountCreditsIgnoreTheKeyReset() throws {
        var key = Self.key(limit: 10, reset: "Weekly")
        key.usageDaily = 3
        let snapshot = OpenRouterSnapshot.build(
            key: key,
            credits: .init(totalCredits: 200, totalUsage: 40),
            activity: [row(date: "2026-01-30", usage: 8)],
            fetchedAt: now
        )
        XCTAssertNil(snapshot.limitReset)
        XCTAssertEqual(snapshot.spendByUTCDay["2026-01-30"] ?? -1, 8, accuracy: 0.001)
        XCTAssertEqual(snapshot.spendByUTCDay["2026-01-31"] ?? -1, 3, accuracy: 0.001)
        let chart = try XCTUnwrap(OpenRouterDailyChart.make(for: snapshot, now: now, calendar: utc))
        XCTAssertEqual(chart.kind, .weekly)
        XCTAssertNil(chart.periodUsedPercent)
        XCTAssertNil(chart.resetsAt)
        let days = chart.days(weekOffset: 0)
        let friday = try XCTUnwrap(days.first { utc.component(.day, from: $0.date) == 30 })
        let saturday = try XCTUnwrap(days.first { utc.component(.day, from: $0.date) == 31 })
        XCTAssertEqual(friday.spentUSD, 8 / 200 * 100, accuracy: 0.001)
        XCTAssertEqual(saturday.spentUSD, 3 / 200 * 100, accuracy: 0.001)
    }

    /// A monthly key limit paces the subscription month and keeps that month's daily percents.
    func testMonthlyKeyLimitUsesTheSubscriptionMonth() throws {
        var key = Self.key(limit: 50, reset: "monthly")
        key.usageMonthly = 20
        let snapshot = OpenRouterSnapshot.build(
            key: key,
            credits: nil,
            activity: [row(date: "2026-01-31", usage: 10)],
            fetchedAt: now
        )
        let chart = try XCTUnwrap(OpenRouterDailyChart.make(for: snapshot, now: now, calendar: utc))
        XCTAssertEqual(chart.kind, .monthly)
        XCTAssertEqual(chart.periodUsedPercent ?? -1, snapshot.usedPercent ?? -1, accuracy: 0.001)
        XCTAssertEqual(chart.resetsAt, utc.date(from: DateComponents(year: 2026, month: 2, day: 1)))
        XCTAssertEqual(chart.periodStart, utc.date(from: DateComponents(year: 2026, month: 1, day: 1)))
        let days = chart.days(weekOffset: 0)
        XCTAssertEqual(days.count, 7)
        XCTAssertEqual(utc.component(.day, from: days[0].date), 26)
        let saturday = try XCTUnwrap(days.first { utc.component(.day, from: $0.date) == 31 })
        XCTAssertEqual(saturday.spentUSD, 10 / 50 * 100, accuracy: 0.001)
    }

    /// An unrecognized reset (hourly) is a cap with no week to pace.
    func testHourlyKeyCapIsNotPacedAsAWeek() throws {
        let snapshot = OpenRouterSnapshot.build(key: Self.key(limit: 80, reset: "hourly"), credits: nil, fetchedAt: now)
        let chart = try XCTUnwrap(OpenRouterDailyChart.make(for: snapshot, now: now, calendar: utc))
        XCTAssertEqual(snapshot.limitReset, "hourly")
        XCTAssertEqual(chart.kind, .weekly)
        XCTAssertNil(chart.periodUsedPercent)
        XCTAssertNil(chart.resetsAt)
    }

    func testNoBudgetHasNoChart() {
        let snapshot = OpenRouterSnapshot.build(key: Self.key(limit: nil, reset: nil), credits: nil, fetchedAt: now)
        XCTAssertNil(OpenRouterDailyChart.make(for: snapshot, now: now, calendar: utc))
    }

    private static func key(limit: Double?, reset: String?) -> OpenRouterKeyData {
        OpenRouterKeyData(
            label: "Test Key",
            usage: 20,
            usageDaily: nil,
            usageWeekly: nil,
            usageMonthly: nil,
            limit: limit,
            limitRemaining: nil,
            limitReset: reset,
            isFreeTier: false,
            isManagementKey: true
        )
    }

    private func row(date: String, usage: Double) -> OpenRouterActivityRow {
        OpenRouterActivityRow(
            date: date,
            model: "test",
            providerName: nil,
            usage: usage,
            requests: 1,
            promptTokens: 1,
            completionTokens: 1,
            reasoningTokens: 0
        )
    }
}
