import SwiftUI
@testable import TokenMon
import XCTest

/// The "This Mac" tag marks local figures and draws nothing for account ones.
@MainActor
final class UsageSourceTagTests: XCTestCase {
    func testLocalFiguresAreTaggedThisMac() {
        let view = UsageSourceTag(source: .thisComputer)
        let size = ViewHarness.renderBothAppearances(view)
        XCTAssertEqual(ViewTree(view).texts, ["laptopcomputer", "This Mac"])
        XCTAssertGreaterThan(size.height, 0)
    }

    func testAccountFiguresAreNotTagged() {
        let view = UsageSourceTag(source: .account)
        let size = ViewHarness.render(view)
        XCTAssertEqual(ViewTree(view).texts, [])
        XCTAssertEqual(size.height, 0)
    }
}
