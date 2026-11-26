import SwiftUI
@testable import TokenMon
import XCTest

/// The 2×2 stats sheet: its cells, the "This Mac" caption, and spoken titles.
@MainActor
final class MetricStatTests: XCTestCase {
    private let stats = [
        MetricStat(title: "Tokens", value: "44M"),
        MetricStat(title: "Requests", value: "1,204"),
        MetricStat(title: "Top model", value: "Opus", monospaced: false),
        MetricStat(title: "Cost", value: "$12.40")
    ]

    func testCellShowsTitleAndValue() {
        let cell = MetricStat(title: "Tokens", value: "44M")
        ViewHarness.render(cell)
        XCTAssertEqual(ViewTree(cell).texts, ["Tokens", "44M"])
        let plain = MetricStat(title: "Top model", value: "Opus", monospaced: false)
        ViewHarness.render(plain)
        XCTAssertEqual(ViewTree(plain).texts, ["Top model", "Opus"])
    }

    /// Local figures add a "This Mac" caption under the cells, which makes the
    /// sheet taller than the same account figures.
    func testLocalSheetIsCaptionedThisMac() {
        let local = MetricStatGrid(source: .thisComputer, stats)
        let account = MetricStatGrid(source: .account, stats)
        let localHeight = ViewHarness.renderBothAppearances(local).height
        let accountHeight = ViewHarness.renderBothAppearances(account).height
        XCTAssertGreaterThan(localHeight, accountHeight)
        XCTAssertTrue(ViewTree(local).shows("This Mac"))
        XCTAssertFalse(ViewTree(account).shows("This Mac"))
        for value in ["44M", "1,204", "Opus", "$12.40"] {
            XCTAssertTrue(ViewTree(account).shows(value), value)
        }
    }

    func testCellsAreSpokenWithTheirLocation() {
        let local = MetricStatGrid(source: .thisComputer, stats)
        XCTAssertEqual(local.spokenTitle(of: stats[0]), "Tokens on this Mac")
        XCTAssertEqual(MetricStatGrid(source: .account, stats).spokenTitle(of: stats[0]), "Tokens")
    }

    /// A sheet of one stat still lays out, with an empty bottom row.
    func testShortSheetRenders() {
        let view = MetricStatGrid(source: .account, Array(stats.prefix(1)))
        XCTAssertGreaterThan(ViewHarness.render(view).height, 0)
        XCTAssertEqual(ViewTree(view).texts.filter { $0 == "44M" }.count, 1)
    }
}
