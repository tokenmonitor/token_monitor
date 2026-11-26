import SwiftUI
@testable import TokenMon
import XCTest

/// Footer pills: the plain pill and the saved-resets count with its hover help.
@MainActor
final class PanelPillTests: XCTestCase {
    func testPillShowsItsLabelAndFigure() {
        let view = PanelPill(label: "Credits", systemImage: "star", value: "12", tint: .green)
        ViewHarness.renderBothAppearances(view)
        XCTAssertEqual(ViewTree(view).texts, ["star", "Credits", "12"])
    }

    /// Without a figure the pill is narrower: the label alone.
    func testPillWithoutFigureShowsOnlyTheLabel() {
        let bare = PanelPill(label: "Update available")
        let withValue = PanelPill(label: "Update available", value: "1.2.3")
        XCTAssertEqual(ViewTree(bare).texts, ["Update available"])
        ViewHarness.render(bare)
        XCTAssertLessThan(ViewHarness.idealSize(bare).width, ViewHarness.idealSize(withValue).width)
    }

    func testSavedResetsPillShowsTheCountAndNextExpiry() throws {
        let expiry = Date(timeIntervalSinceReferenceDate: 815_000_000)
        let resets = try XCTUnwrap(SavedResets(count: 3, nextExpiry: expiry))
        let view = SavedResetsPill(resets: resets, tint: .orange)
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Resets available"))
        XCTAssertTrue(tree.texts.contains("3"))
        let expected = "Next expires \(Format.resetDate(expiry, dateFormat: "EEE dd MMMM h:mma"))"
        XCTAssertTrue(tree.shows(expected), "\(tree.texts)")
    }

    /// A provider that reports no expiry gets a generic help tag.
    func testSavedResetsPillWithoutExpiryHasAGenericHelpTag() throws {
        let view = try SavedResetsPill(resets: XCTUnwrap(SavedResets(count: 1)), tint: .orange)
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Saved usage-limit resets"), "\(tree.texts)")
        XCTAssertFalse(tree.shows("Next expires"))
    }

    func testSavedResetsPillDrawsNothingWithoutAnInventory() {
        let view = SavedResetsPill(resets: nil, tint: .orange)
        XCTAssertEqual(ViewHarness.render(view).height, 0)
        XCTAssertEqual(ViewTree(view).texts, [])
    }
}
