import SwiftUI
@testable import TokenMon
import XCTest

/// The stale-data caption: the failure, then how old the data shown is.
@MainActor
final class StaleDataCaptionTests: XCTestCase {
    func testCaptionShowsTheFailureAndTheDataAge() {
        let caption = StaleDataCaption(message: "Refresh failed: offline", lastRefreshedAt: Date().addingTimeInterval(-600))
        let bare = StaleDataCaption(message: "Refresh failed: offline", lastRefreshedAt: nil)
        XCTAssertGreaterThan(ViewHarness.renderBothAppearances(caption).height, ViewHarness.render(bare).height)
        XCTAssertTrue(ViewTree(caption).shows("Refresh failed: offline"))
        XCTAssertTrue(ViewTree(caption).shows("Showing data from"))
    }

    /// Without a last refresh only the failure is shown.
    func testCaptionWithoutARefreshShowsOnlyTheFailure() {
        let caption = StaleDataCaption(message: "Refresh failed: offline", lastRefreshedAt: nil)
        XCTAssertEqual(ViewTree(caption).texts, ["Refresh failed: offline"])
    }
}
