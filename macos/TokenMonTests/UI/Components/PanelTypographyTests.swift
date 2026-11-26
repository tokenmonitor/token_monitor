import SwiftUI
@testable import TokenMon
import XCTest

/// Section headers and the shared caption styles.
@MainActor
final class PanelTypographyTests: XCTestCase {
    func testSectionHeaderShowsItsTitle() {
        let view = PanelSectionHeader(title: "Usage Breakdown")
        ViewHarness.renderBothAppearances(view)
        XCTAssertEqual(ViewTree(view).texts, ["Usage Breakdown"])
    }

    func testHeaderRowShowsTheTrailingFigureWhenGiven() {
        let row = PanelSectionHeaderRow(title: "Weekly", trailing: "42% used")
        ViewHarness.render(row)
        XCTAssertEqual(ViewTree(row).texts, ["Weekly", "42% used"])
        let bare = PanelSectionHeaderRow(title: "Weekly")
        ViewHarness.render(bare)
        XCTAssertEqual(ViewTree(bare).texts, ["Weekly"])
    }

    /// The meta label renders in both its plain and uppercased forms; the
    /// uppercased form is wider (tracked capitals).
    func testMetaLabelStyles() {
        let plain = Text("— weekly").panelMetaLabel()
        let upper = Text("— weekly").panelMetaLabel(uppercased: true)
        ViewHarness.renderBothAppearances(plain)
        ViewHarness.renderBothAppearances(upper)
        let plainWidth = ViewHarness.idealSize(plain).width
        let upperWidth = ViewHarness.idealSize(upper).width
        XCTAssertGreaterThan(upperWidth, plainWidth)
        XCTAssertTrue(ViewTree(upper).shows("— weekly"))
        // The style on its own defaults to the plain form.
        XCTAssertFalse(PanelMetaLabelStyle().uppercased)
        let styled = Text("— weekly").modifier(PanelMetaLabelStyle())
        XCTAssertEqual(ViewHarness.idealSize(styled).width, plainWidth, accuracy: 0.5)
    }

    func testResetCaptionKeepsItsText() {
        let view = Text("Resets Thursday").resetCaption()
        XCTAssertGreaterThan(ViewHarness.renderBothAppearances(view).height, 0)
        XCTAssertTrue(ViewTree(view).shows("Resets Thursday"))
    }

    func testChartStemGeometry() {
        XCTAssertEqual(PanelChartStem.height, 68)
        XCTAssertEqual(PanelChartStem.width, 12)
        XCTAssertEqual(PanelChartStem.cornerRadius, 5)
    }
}
