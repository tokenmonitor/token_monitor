import SwiftUI
@testable import TokenMon
import XCTest

/// The collapsible header toggles its section and speaks its title.
@MainActor
final class CollapsibleSectionHeaderTests: XCTestCase {
    private final class Box {
        var isExpanded: Bool

        init(_ value: Bool) {
            isExpanded = value
        }
    }

    private func header(_ box: Box, source: UsageSource) -> CollapsibleSectionHeader {
        CollapsibleSectionHeader(
            title: "Models This Month",
            source: source,
            isExpanded: Binding(get: { box.isExpanded }, set: { box.isExpanded = $0 })
        )
    }

    func testPressingTheHeaderTogglesTheSection() {
        let box = Box(false)
        let view = header(box, source: .account)
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        tree.press("Models This Month")
        XCTAssertTrue(box.isExpanded)
        tree.press("Models This Month")
        XCTAssertFalse(box.isExpanded)
    }

    /// A local section carries the "This Mac" tag and says so to VoiceOver.
    func testLocalSectionIsTaggedAndSpokenWithItsLocation() {
        let box = Box(true)
        let view = header(box, source: .thisComputer)
        ViewHarness.render(view)
        XCTAssertTrue(ViewTree(view).shows("This Mac"))
        XCTAssertEqual(view.spokenTitle, "Models This Month on this Mac")
        XCTAssertEqual(header(box, source: .account).spokenTitle, "Models This Month")
    }

    func testAccessibilityValueFollowsTheExpansion() {
        XCTAssertTrue(ViewTree(header(Box(true), source: .account)).shows("Expanded"))
        XCTAssertTrue(ViewTree(header(Box(false), source: .account)).shows("Collapsed"))
    }
}
