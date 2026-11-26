import SwiftUI
@testable import TokenMon
import XCTest

/// The SuperGrok daily chart: header, range, footer per state and arrows.
@MainActor
final class DailyUsageChartViewTests: XCTestCase {
    private let calendar = Calendar.current

    /// A week starting five days ago (today is its sixth day, tomorrow its
    /// last), with `percents` of the weekly pool per day.
    private func week(_ percents: [Double], hasDailyData: Bool = true, isEstimated: Bool = false) -> DailyUsageWeek {
        let today = calendar.startOfDay(for: Date())
        let start = calendar.date(byAdding: .day, value: -5, to: today) ?? today
        let days = percents.enumerated().map { offset, percent in
            let date = calendar.date(byAdding: .day, value: offset, to: start) ?? start
            return DailyUsageDay(
                dayStart: date,
                weekdaySymbol: "Day\(offset)",
                dayOfMonth: "\(offset + 1)",
                segments: percent > 0
                    ? [DailyUsageSegment(productID: "chat", displayName: "Chat", percentOfWeekly: percent, colorToken: .chat)]
                    : [],
                isToday: offset == 5,
                isAfterReset: false
            )
        }
        return DailyUsageWeek(
            weekStart: start,
            weekEnd: calendar.date(byAdding: .day, value: 6, to: start) ?? start,
            days: days,
            legendProducts: [],
            hasDailyData: hasDailyData,
            isEstimated: isEstimated
        )
    }

    /// Six elapsed days earn 6 x 100/7 = 85.7%; 90% used is 4.3% over.
    func testFooterPacesTheLiveWeeklyPercent() {
        let chart = DailyUsageChartView(week: week([10, 20, 15, 15, 10, 20, 0]), periodUsedPercent: 90)
        ViewHarness.renderBothAppearances(chart)
        let tree = ViewTree(chart)
        XCTAssertTrue(tree.shows("Usage 4.3% over today's allowance"), "\(tree.texts)")
        XCTAssertTrue(tree.shows("Daily Budget"))
        XCTAssertTrue(tree.shows("14.3%"))
        XCTAssertTrue(tree.shows(week([]).rangeLabel))
    }

    /// Estimated bars, or a week without daily deltas, say the bars catch up
    /// between polls.
    func testEstimatedBarsSayTheyUpdateBetweenPolls() {
        for chart in [
            DailyUsageChartView(week: week([5, 5, 5, 5, 5, 5, 0], isEstimated: true)),
            DailyUsageChartView(week: week([0, 0, 0, 0, 0, 0, 0], hasDailyData: false))
        ] {
            ViewHarness.render(chart)
            XCTAssertTrue(ViewTree(chart).shows("Bars update between polls · week totals above"))
        }
    }

    /// Real daily data without a live percent needs no footer.
    func testNoFooterForRealDataWithoutPace() {
        let chart = DailyUsageChartView(week: week([5, 5, 5, 5, 5, 5, 0]))
        let paced = DailyUsageChartView(week: week([5, 5, 5, 5, 5, 5, 0]), periodUsedPercent: 30)
        XCTAssertLessThan(ViewHarness.render(chart).height, ViewHarness.render(paced).height)
        XCTAssertFalse(ViewTree(chart).shows("Bars update"))
    }

    func testArrowsRunTheirActions() {
        var moves: [String] = []
        let chart = DailyUsageChartView(
            week: week([1, 2, 3, 4, 5, 6, 0]),
            onPreviousWeek: { moves.append("back") },
            onNextWeek: { moves.append("forward") }
        )
        let tree = ViewTree(chart)
        tree.press("chevron.left")
        tree.press("chevron.right")
        XCTAssertEqual(moves, ["back", "forward"])
    }

    /// A day's bar fills its share of the equal daily cap, capped at full.
    func testFillFractionUsesTheDailyCap() {
        XCTAssertEqual(DailyUsageChartView.fillFraction(forDayUsage: 100.0 / 14), 0.5, accuracy: 0.0001)
        XCTAssertEqual(DailyUsageChartView.fillFraction(forDayUsage: 50), 1)
        XCTAssertEqual(DailyUsageChartView.fillFraction(forDayUsage: 0.01), 0)
    }
}
