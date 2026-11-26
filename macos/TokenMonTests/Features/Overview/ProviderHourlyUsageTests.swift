@testable import TokenMon
import XCTest

/// `ProviderDayHourlyUsage.build` degrades gracefully on short/long input
/// arrays.
final class ProviderHourlyUsageTests: XCTestCase {
    func testShortArraysArePaddedToTwentyFourHours() {
        let usage = ProviderDayHourlyUsage.build(
            dayStart: Date(),
            grokHourWeights: [5.0],
            openCodeGoHourWeights: [],
            openCodeZenHourWeights: [1.0, 2.0]
        )
        XCTAssertEqual(usage.hours.count, 24)
        XCTAssertEqual(usage.hours[0].grokSharePercent, 5.0 / 6.0 * 100, accuracy: 0.001)
        XCTAssertEqual(usage.hours[0].activity, 6, accuracy: 0.001)
        XCTAssertEqual(usage.hours[1].activity, 2, accuracy: 0.001)
        XCTAssertEqual(usage.hours[23].activity, 0, accuracy: 0.001)
    }

    func testLongArraysAreTruncatedToTwentyFourHours() {
        let usage = ProviderDayHourlyUsage.build(
            dayStart: Date(),
            grokHourWeights: Array(repeating: 1.0, count: 30),
            openCodeGoHourWeights: [],
            openCodeZenHourWeights: []
        )
        XCTAssertEqual(usage.hours.count, 24)
        XCTAssertEqual(usage.hours[23].activity, 1, accuracy: 0.001)
    }

    /// Each provider's slice of an hour is its share of the combined activity;
    /// negative inputs count as zero and token arrays are padded or truncated.
    func testEveryProviderGetsItsShareOfTheHour() {
        let usage = ProviderDayHourlyUsage.build(
            dayStart: Date(),
            grokHourWeights: [1],
            openCodeGoHourWeights: [2],
            openCodeZenHourWeights: [3],
            cursorHourWeights: [4],
            claudeHourWeights: [5, -2],
            grokbotHourWeights: [5],
            grokHourTokens: [10],
            openCodeGoHourTokens: Array(repeating: 7, count: 30),
            openCodeZenHourTokens: [-4],
            cursorHourTokens: [30],
            claudeHourTokens: Array(repeating: 9, count: 25)
        )
        let hour = usage.hours[0]
        XCTAssertEqual(hour.activity, 20, accuracy: 0.001)
        XCTAssertEqual(hour.grokActivity, 1, accuracy: 0.001)
        XCTAssertEqual(hour.openCodeGoActivity, 2, accuracy: 0.001)
        XCTAssertEqual(hour.openCodeZenActivity, 3, accuracy: 0.001)
        XCTAssertEqual(hour.cursorActivity, 4, accuracy: 0.001)
        XCTAssertEqual(hour.claudeActivity, 5, accuracy: 0.001)
        XCTAssertEqual(hour.grokbotActivity, 5, accuracy: 0.001)
        XCTAssertEqual(hour.grokTokens, 10)
        XCTAssertEqual(hour.openCodeGoTokens, 7)
        XCTAssertEqual(hour.openCodeZenTokens, 0)
        XCTAssertEqual(hour.cursorTokens, 30)
        XCTAssertEqual(hour.claudeTokens, 9)
        XCTAssertEqual(usage.hours[1].activity, 0, accuracy: 0.001)
        XCTAssertEqual(usage.hours[1].grokTokens, 0)
        XCTAssertEqual(usage.hours[23].openCodeGoTokens, 7)
        XCTAssertEqual(hour.id, 0)
        XCTAssertEqual(hour.hourLabel, Format.hourLabel(for: 0))
        XCTAssertTrue(hour.hasActivity)
        XCTAssertFalse(usage.hours[1].hasActivity)
        XCTAssertEqual(usage.maxActivity, 20, accuracy: 0.001)
        XCTAssertFalse(usage.isEmpty)
    }

    /// Without token arrays Grok and Claude report quota only.
    func testQuotaOnlyProvidersHaveNoTokens() {
        let usage = ProviderDayHourlyUsage.build(
            dayStart: Date(),
            grokHourWeights: [],
            openCodeGoHourWeights: [],
            openCodeZenHourWeights: []
        )
        XCTAssertNil(usage.hours[0].grokTokens)
        XCTAssertNil(usage.hours[0].claudeTokens)
        XCTAssertTrue(usage.isEmpty)
        XCTAssertEqual(usage.maxActivity, 0, accuracy: 0.001)
        XCTAssertTrue(ProviderDayHourlyUsage(dayStart: Date(), hours: []).isEmpty)
    }

    /// The Overview chart's schedule starts at today's midnight, so the first
    /// render shows today, then has one entry per following local midnight.
    func testMidnightScheduleStartsAtTodayThenEachLocalMidnight() throws {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = try XCTUnwrap(TimeZone(identifier: "America/Denver"))
        let lateEvening = try XCTUnwrap(calendar.date(from: DateComponents(year: 2026, month: 3, day: 7, hour: 23, minute: 59)))
        let midnights = OverviewPanelView.midnightSchedule(from: lateEvening, calendar: calendar)
        XCTAssertEqual(midnights.count, 8)
        XCTAssertEqual(midnights.first, calendar.date(from: DateComponents(year: 2026, month: 3, day: 7)))
        XCTAssertLessThanOrEqual(try XCTUnwrap(midnights.first), lateEvening)
        XCTAssertEqual(midnights[1], calendar.date(from: DateComponents(year: 2026, month: 3, day: 8)))
        // DST starts on 2026-03-08 in Denver: still one entry per local midnight.
        XCTAssertEqual(midnights[2], calendar.date(from: DateComponents(year: 2026, month: 3, day: 9)))
        XCTAssertTrue(midnights.allSatisfy { calendar.component(.hour, from: $0) == 0 })
    }
}
