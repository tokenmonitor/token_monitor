import SwiftUI
@testable import TokenMon
import XCTest

/// The daily-budget bars: header, range, pace footer per state, week arrows,
/// and the navigable card's week switching.
@MainActor
final class DailyBudgetBarsViewTests: XCTestCase {
    private final class Recorder {
        var indices: [Int] = []

        /// Notes that the stem for `index` was built; returns `index`.
        func built(_ index: Int) -> Int {
            indices.append(index)
            return index
        }
    }

    private let calendar = Calendar.current

    private var today: Date {
        calendar.startOfDay(for: Date())
    }

    /// A day `offset` days from today with a 10%-a-day allowance.
    private func day(_ offset: Int, spent: Double, budget: Double = 10, prior: Bool = false) -> DailyBudgetDay {
        DailyBudgetDay(
            date: calendar.date(byAdding: .day, value: offset, to: today) ?? today,
            spentUSD: spent,
            budgetUSD: budget,
            isPriorWindow: prior
        )
    }

    /// The week ending today, `spent` per day.
    private func week(_ spent: [Double]) -> [DailyBudgetDay] {
        spent.enumerated().map { day($0.offset - (spent.count - 1), spent: $0.element) }
    }

    // MARK: - Footer

    /// With a live used % the footer paces it: 7 days earned 70%, 80% used.
    func testWeeklyFooterPacesTheLiveUsedPercent() {
        let view = WeeklyDailyBudgetBarsView(days: week([10, 10, 10, 10, 10, 10, 20]), accent: .orange, periodUsedPercent: 80)
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Usage 10.0% over today's allowance"), "\(tree.texts)")
        XCTAssertTrue(tree.shows("Daily Budget"))
        XCTAssertTrue(tree.shows("10.0%"))
    }

    /// A monthly chart with a known start paces from it.
    func testMonthlyFooterPacesFromTheKnownStart() {
        let days = week([5, 5, 5, 5, 5, 5, 5])
        let view = DailyBudgetBarsView(
            days: days,
            accent: .orange,
            allowancePeriod: .monthly,
            periodUsedPercent: 35,
            periodStart: days[0].date
        )
        ViewHarness.render(view)
        XCTAssertTrue(ViewTree(view).shows("35.0% usage left today from unused prior days"), "\(ViewTree(view).texts)")
    }

    /// A weekly chart whose bars are all prior-window days has no pace start,
    /// so the footer falls back to the bars: nothing spent yet.
    func testWeeklyWithoutCurrentDaysFallsBackToTheBars() {
        let days = (0..<3).map { day($0 - 2, spent: 0, prior: true) }
        let view = DailyBudgetBarsView(days: days, accent: .orange, allowancePeriod: .weekly, periodUsedPercent: 20)
        ViewHarness.render(view)
        XCTAssertTrue(ViewTree(view).shows("No usage yet · 10.0%/day"))
    }

    /// A monthly chart without a start or reset cannot pace, so today's
    /// overrun comes from the bars.
    func testMonthlyWithoutStartFlagsTodaysOverrun() {
        let view = DailyBudgetBarsView(days: week([2, 4, 12]), accent: .orange, allowancePeriod: .monthly, periodUsedPercent: 18)
        ViewHarness.render(view)
        XCTAssertTrue(ViewTree(view).shows("Over today's allowance"))
    }

    /// Spending within today's allowance and no live percent: no footer.
    func testNoFooterWhenTodayIsWithinItsAllowance() {
        let view = DailyBudgetBarsView(days: week([2, 4, 3]), accent: .orange)
        let withFooter = DailyBudgetBarsView(days: week([2, 4, 12]), accent: .orange)
        XCTAssertLessThan(ViewHarness.render(view).height, ViewHarness.render(withFooter).height)
        XCTAssertFalse(ViewTree(view).shows("allowance"))
    }

    /// Past weeks drop the live footer.
    func testPastWeekHasNoFooter() {
        let view = WeeklyDailyBudgetBarsView(days: week([0, 0, 0]), accent: .orange, showsLiveFooter: false)
        ViewHarness.render(view)
        XCTAssertFalse(ViewTree(view).shows("No usage yet"))
    }

    // MARK: - Header and bars

    func testRangeLabelWithinOneMonth() throws {
        let first = try XCTUnwrap(calendar.date(from: DateComponents(year: 2026, month: 3, day: 2)))
        let days = (0..<7).map { DailyBudgetDay(date: first.addingTimeInterval(Double($0) * 86_400 + 3600), spentUSD: 1, budgetUSD: 10) }
        let view = DailyBudgetBarsView(days: days, accent: .orange, showsLiveFooter: false)
        ViewHarness.render(view)
        XCTAssertTrue(ViewTree(view).shows("\(Format.date(days[0].date, dateFormat: "MMMM d")) – 8"), "\(ViewTree(view).texts)")
    }

    func testRangeLabelAcrossTwoMonths() throws {
        let first = try XCTUnwrap(calendar.date(from: DateComponents(year: 2026, month: 3, day: 29, hour: 1)))
        let days = (0..<7).map { DailyBudgetDay(date: first.addingTimeInterval(Double($0) * 86_400), spentUSD: 1, budgetUSD: 10) }
        let view = DailyBudgetBarsView(days: days, accent: .orange, showsLiveFooter: false)
        ViewHarness.render(view)
        let last = days[6].date.formatted(Date.FormatStyle().month(.abbreviated).day())
        XCTAssertTrue(ViewTree(view).shows("\(Format.date(days[0].date, dateFormat: "MMMM d")) – \(last)"))
    }

    /// No days: no range and no daily share in the header.
    func testEmptyChartHasNoRangeOrShare() {
        let view = DailyBudgetBarsView(days: [], accent: .orange, showsLiveFooter: false)
        ViewHarness.render(view)
        XCTAssertEqual(ViewTree(view).texts, ["Daily Budget", ""])
    }

    /// Each bar is captioned with its rounded percent and two-letter weekday;
    /// future, prior-window and tiny days render too.
    func testBarCaptionsShowPercentAndWeekday() {
        let days = [day(-2, spent: 4, prior: true), day(-1, spent: 0.001), day(0, spent: 12.6), day(1, spent: 0)]
        let view = DailyBudgetBarsView(days: days, accent: .orange, periodUsedPercent: 10)
        XCTAssertGreaterThan(ViewHarness.renderBothAppearances(view).height, PanelChartStem.height)
        XCTAssertEqual(days.map { DailyBudgetBarsView.captionLines($0).percent }, ["4%", "0%", "13%", "0%"])
        XCTAssertEqual(DailyBudgetBarsView.captionLines(day(0, spent: -3)).percent, "0%")
    }

    func testBarCaptionNamesTheWeekday() throws {
        let monday = try XCTUnwrap(calendar.date(from: DateComponents(year: 2026, month: 3, day: 2, hour: 9)))
        let weekday = DailyBudgetBarsView.captionLines(DailyBudgetDay(date: monday, spentUSD: 1, budgetUSD: 10)).weekday
        XCTAssertEqual(weekday.count, 2)
        if Locale.current.language.languageCode == .english {
            XCTAssertEqual(weekday, "Mo")
        }
    }

    /// A row opened already hovered draws the outline and the floating label.
    func testHoveredBarDrawsItsLabel() {
        let days = week([3, 12, 6])
        let stems = Recorder()
        let row = DailyBarsRow(days: days, allowanceNoun: "weekly", initialHover: (index: 1, pointer: CGPoint(x: 170, y: 30))) { index in
            Color.orange.opacity(stems.built(index) >= 0 ? 1 : 0)
        } caption: { index in
            Text("\(index)")
        }
        let plain = DailyBarsRow(days: days, allowanceNoun: "weekly") { _ in Color.orange } caption: { Text("\($0)") }
        XCTAssertEqual(ViewHarness.render(row).height, ViewHarness.render(plain).height)
        XCTAssertEqual(Set(stems.indices), [0, 1, 2])
        XCTAssertNotEqual(ViewHarness.pixels(row), ViewHarness.pixels(plain))
        // A hover index outside the days draws no label.
        let stale = DailyBarsRow(days: days, allowanceNoun: "weekly", initialHover: (index: 9, pointer: CGPoint(x: 170, y: 30))) { _ in
            Color.orange
        } caption: { Text("\($0)") }
        XCTAssertEqual(ViewHarness.pixels(stale), ViewHarness.pixels(plain))
    }

    /// Future days' hover label leaves out the usage line.
    func testHoveredFutureDayRenders() {
        let days = [day(0, spent: 3), day(1, spent: 0)]
        let row = DailyBarsRow(days: days, allowanceNoun: "weekly", initialHover: (index: 1, pointer: CGPoint(x: 250, y: 30))) { _ in
            Color.orange
        } caption: { Text("\($0)") }
        let plain = DailyBarsRow(days: days, allowanceNoun: "weekly") { _ in Color.orange } caption: { Text("\($0)") }
        XCTAssertNotEqual(ViewHarness.pixels(row), ViewHarness.pixels(plain))
    }

    // MARK: - Week arrows

    func testArrowsRunTheirActions() {
        var moves: [String] = []
        let view = DailyBudgetBarsView(
            days: week([1, 2]),
            accent: .orange,
            onPreviousWeek: { moves.append("back") },
            onNextWeek: { moves.append("forward") },
            canGoNext: true
        )
        ViewHarness.render(view)
        let tree = ViewTree(view)
        tree.press("chevron.left")
        tree.press("chevron.right")
        XCTAssertEqual(moves, ["back", "forward"])
    }

    /// Without a previous-week action the back arrow is disabled and does nothing.
    func testArrowsWithoutActionsDoNothing() {
        let nav = WeekChartNavigation()
        ViewHarness.renderBothAppearances(nav)
        let tree = ViewTree(nav)
        tree.press("chevron.left")
        tree.press("chevron.right")
        XCTAssertEqual(tree.buttons.count, 2)
        XCTAssertFalse(nav.canGoNext)
    }

    // MARK: - Navigable card

    func testNavigableWeeklyCardShowsTheCurrentWeekWithPace() {
        var asked: [Int] = []
        let days = week([10, 10, 10, 10, 10, 10, 20])
        let card = NavigableDailyBudgetCard(style: .weekly, accent: .orange, periodUsedPercent: 80) { offset in
            asked.append(offset)
            return days
        }
        ViewHarness.renderBothAppearances(card)
        let tree = ViewTree(card)
        XCTAssertTrue(tree.shows("Usage 10.0% over today's allowance"))
        XCTAssertEqual(Set(asked), [0])
        // The arrows' actions move the shown week (state lives in the render graph).
        tree.press("chevron.left")
        tree.press("chevron.right")
    }

    /// An earlier week drops the live percent and footer.
    func testNavigableMonthlyCardShowsAnEarlierWeekWithoutPace() {
        var asked: [Int] = []
        let days = week([10, 10, 10, 10, 10, 10, 20])
        let card = NavigableDailyBudgetCard(
            style: .monthly,
            accent: .orange,
            periodUsedPercent: 80,
            periodStart: days[0].date,
            resetsAt: today.addingTimeInterval(20 * 86_400),
            weekOffset: -1
        ) { offset in
            asked.append(offset)
            return days
        }
        ViewHarness.render(card)
        let tree = ViewTree(card)
        XCTAssertFalse(tree.shows("allowance"))
        XCTAssertTrue(tree.shows("Daily Budget"))
        XCTAssertEqual(Set(asked), [-1])
        tree.press("chevron.right")
    }

    func testNavigableCardWithoutDaysDrawsNothing() {
        let card = NavigableDailyBudgetCard(style: .weekly, accent: .orange) { _ in [] }
        XCTAssertEqual(ViewHarness.render(card).height, 0)
        XCTAssertEqual(ViewTree(card).texts, [])
    }
}
