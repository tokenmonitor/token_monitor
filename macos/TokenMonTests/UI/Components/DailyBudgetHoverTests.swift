import SwiftUI
@testable import TokenMon
import XCTest

/// Pointer and placement math for the daily bars' floating hover label.
final class DailyBudgetHoverTests: XCTestCase {
    func testColumnIndexMapsThePointerToABar() {
        XCTAssertEqual(DailyBudgetHover.columnIndex(x: 0, columnWidth: 40, columns: 7), 0)
        XCTAssertEqual(DailyBudgetHover.columnIndex(x: 119.9, columnWidth: 40, columns: 7), 2)
        XCTAssertEqual(DailyBudgetHover.columnIndex(x: 279, columnWidth: 40, columns: 7), 6)
        XCTAssertNil(DailyBudgetHover.columnIndex(x: 280, columnWidth: 40, columns: 7))
        XCTAssertNil(DailyBudgetHover.columnIndex(x: -1, columnWidth: 40, columns: 7))
        XCTAssertNil(DailyBudgetHover.columnIndex(x: 10, columnWidth: 0, columns: 0))
    }

    /// Only the stem (plus a little slack) shows the label; the gaps between
    /// stems hide it.
    func testStemIndexCoversOnlyTheStem() {
        let slop = DailyBudgetHover.stemHitSlop
        // Column 1 spans 40..<80 with its 12-wide stem centered on 60.
        XCTAssertEqual(DailyBudgetHover.stemIndex(x: 60, columnWidth: 40, columns: 7, stemWidth: 12), 1)
        XCTAssertEqual(DailyBudgetHover.stemIndex(x: 54 - slop, columnWidth: 40, columns: 7, stemWidth: 12), 1)
        XCTAssertEqual(DailyBudgetHover.stemIndex(x: 66 + slop, columnWidth: 40, columns: 7, stemWidth: 12), 1)
        XCTAssertNil(DailyBudgetHover.stemIndex(x: 53 - slop, columnWidth: 40, columns: 7, stemWidth: 12))
        XCTAssertNil(DailyBudgetHover.stemIndex(x: 67 + slop, columnWidth: 40, columns: 7, stemWidth: 12))
        XCTAssertNil(DailyBudgetHover.stemIndex(x: 41, columnWidth: 40, columns: 7, stemWidth: 12))
        XCTAssertNil(DailyBudgetHover.stemIndex(x: 300, columnWidth: 40, columns: 7, stemWidth: 12))
    }

    /// The row's stems split its width into equal columns: 280 wide with 7
    /// days gives 40-wide columns, so 60 is over the second stem.
    func testStemIndexFromThePointerAndRowSize() {
        let size = CGSize(width: 280, height: 68)
        XCTAssertEqual(DailyBudgetHover.stemIndex(at: CGPoint(x: 60, y: 30), in: size, columns: 7), 1)
        XCTAssertEqual(DailyBudgetHover.stemIndex(at: CGPoint(x: 260, y: 0), in: size, columns: 7), 6)
        XCTAssertNil(DailyBudgetHover.stemIndex(at: CGPoint(x: 41, y: 30), in: size, columns: 7))
        XCTAssertNil(DailyBudgetHover.stemIndex(at: CGPoint(x: 60, y: 30), in: size, columns: 0))
    }

    /// The label floats left of the pointer, flips right when there is no
    /// room on the left, and stays inside the chart when neither side fits.
    func testLabelFloatsBesideThePointer() {
        let size = CGSize(width: 100, height: 60)
        let gap = HoverLabelPlacement.pointerGap
        XCTAssertEqual(
            HoverLabelPlacement.labelOrigin(pointer: CGPoint(x: 200, y: 50), labelSize: size, containerWidth: 300),
            CGPoint(x: 200 - gap - 100, y: 20)
        )
        XCTAssertEqual(
            HoverLabelPlacement.labelOrigin(pointer: CGPoint(x: 40, y: 50), labelSize: size, containerWidth: 300),
            CGPoint(x: 40 + gap, y: 20)
        )
        XCTAssertEqual(
            HoverLabelPlacement.labelOrigin(pointer: CGPoint(x: 100, y: 50), labelSize: CGSize(width: 250, height: 60), containerWidth: 300),
            CGPoint(x: 0, y: 20)
        )
    }

    func testUsageLineDescribesTheDay() {
        let date = Date(timeIntervalSinceReferenceDate: 0)
        XCTAssertEqual(
            DailyBudgetHover.usageLine(DailyBudgetDay(date: date, spentUSD: 31, budgetUSD: 100.0 / 7), allowanceNoun: "weekly"),
            "31.0% of weekly · 217% of daily allowance"
        )
        XCTAssertEqual(
            DailyBudgetHover.usageLine(DailyBudgetDay(date: date, spentUSD: 0, budgetUSD: 100.0 / 7), allowanceNoun: "weekly"),
            "0% of the 14.3% daily allowance"
        )
        XCTAssertEqual(
            DailyBudgetHover.usageLine(
                DailyBudgetDay(date: date, spentUSD: 5, budgetUSD: 100.0 / 30, isPriorWindow: true),
                allowanceNoun: "monthly"
            ),
            "5.0% of the previous monthly pool"
        )
        XCTAssertEqual(
            DailyBudgetHover.usageLine(DailyBudgetDay(date: date, spentUSD: 5, budgetUSD: 0), allowanceNoun: "weekly"),
            "No budget"
        )
    }

    // MARK: - Label

    private func tokens(models: [String]) -> DailyTokenUsage {
        var usage = DailyTokenUsage(source: .thisComputer)
        for (index, model) in models.enumerated() {
            usage.add(model: model, input: Int64(1_000 * (index + 1)), output: 500, costUSD: 1.5)
        }
        return usage
    }

    /// A past day lists its date, budget line, tokens with value, the token
    /// breakdown, and its top three models.
    @MainActor
    func testLabelShowsTheDaysUsageAndTopModels() {
        let date = Date(timeIntervalSinceReferenceDate: 815_000_000)
        let day = DailyBudgetDay(
            date: date,
            spentUSD: 31,
            budgetUSD: 100.0 / 7,
            tokens: tokens(models: ["A", "B", "C", "D"])
        )
        let label = DailyBudgetHoverLabel(day: day, allowanceNoun: "weekly")
        ViewHarness.renderBothAppearances(label)
        let tree = ViewTree(label)
        XCTAssertTrue(tree.shows(Format.date(date, dateFormat: "EEEE, MMM d")))
        XCTAssertTrue(tree.shows("31.0% of weekly · 217% of daily allowance"))
        XCTAssertTrue(tree.shows("tokens on this Mac"))
        XCTAssertTrue(tree.shows("≈ $6.00"))
        XCTAssertTrue(tree.shows("Cache write"))
        XCTAssertTrue(tree.texts.contains("D"))
        XCTAssertTrue(tree.texts.contains("B"))
        XCTAssertFalse(tree.texts.contains("A"), "only the top three models are listed")
    }

    /// A future day shows only its date; a day without tokens has no token
    /// section; tokens without model names list no models.
    @MainActor
    func testLabelLeavesOutWhatTheDayLacks() {
        let date = Date(timeIntervalSinceReferenceDate: 815_000_000)
        let future = DailyBudgetHoverLabel(day: DailyBudgetDay(date: date, spentUSD: 0, budgetUSD: 10), allowanceNoun: "weekly", isFuture: true)
        ViewHarness.render(future)
        XCTAssertEqual(ViewTree(future).texts, [Format.date(date, dateFormat: "EEEE, MMM d")])

        let plain = DailyBudgetHoverLabel(day: DailyBudgetDay(date: date, spentUSD: 5, budgetUSD: 10), allowanceNoun: "monthly")
        ViewHarness.render(plain)
        XCTAssertFalse(ViewTree(plain).shows("tokens"))

        var unnamed = DailyTokenUsage(source: .account)
        unnamed.add(model: "", output: 2_000, costUSD: nil)
        let anonymous = DailyBudgetHoverLabel(
            day: DailyBudgetDay(date: date, spentUSD: 5, budgetUSD: 10, tokens: unnamed),
            allowanceNoun: "monthly"
        )
        ViewHarness.render(anonymous)
        let tree = ViewTree(anonymous)
        XCTAssertTrue(tree.texts.contains("tokens"))
        XCTAssertFalse(tree.shows("≈"))
        XCTAssertEqual(tree.texts.last, "0", "the breakdown ends the label")
    }
}
