import SwiftUI
@testable import TokenMon
import XCTest

/// The slim used-percent track and the limit caption under it.
@MainActor
final class SlimUsageTrackTests: XCTestCase {
    func testTrackShowsLabelPercentAndCaption() {
        let view = SlimUsageTrack(label: "Weekly", percent: 41.6, color: .blue, caption: "Resets Thursday")
        ViewHarness.renderBothAppearances(view)
        XCTAssertEqual(ViewTree(view).texts, ["Weekly", "42% Used", "Resets Thursday"])
    }

    /// The percent is clamped to 0…100 for display.
    func testPercentIsClamped() {
        XCTAssertEqual(SlimUsageTrack.usedText(140), "100% Used")
        XCTAssertEqual(SlimUsageTrack.usedText(-3), "0% Used")
        XCTAssertEqual(SlimUsageTrack.usedText(99.5), "100% Used")
    }

    /// Inside a card whose header names the pool, the label row is hidden and
    /// the track is shorter.
    func testHiddenLabelLeavesOnlyTheBar() {
        let labelled = SlimUsageTrack(label: "Weekly", percent: 10, color: .blue)
        let bare = SlimUsageTrack(label: "Weekly", percent: 10, color: .blue, showsLabel: false)
        XCTAssertEqual(ViewTree(bare).texts, [])
        XCTAssertLessThan(ViewHarness.render(bare).height, ViewHarness.render(labelled).height)
    }

    /// An exhausted pool with a future reset counts down to it, inside a
    /// timeline that refreshes the countdown.
    func testExhaustedPoolCountsDownToTheReset() {
        let reset = Date().addingTimeInterval(2 * 3600 + 5 * 60 + 30)
        let caption = UsageLimitCaption(resetsAt: reset, percent: 100, normalCaption: "ignored")
        XCTAssertGreaterThan(ViewHarness.render(caption).height, 0)
        let track = SlimUsageTrack(label: "Weekly", percent: 100, color: .red, resetsAt: reset)
        let plain = SlimUsageTrack(label: "Weekly", percent: 100, color: .red)
        XCTAssertGreaterThan(ViewHarness.render(track).height, 0)
        XCTAssertGreaterThanOrEqual(ViewHarness.render(track).height, ViewHarness.render(plain).height)
    }

    /// A past reset at 100% says so instead of counting down.
    func testExhaustedPoolWithPassedReset() {
        let caption = UsageLimitCaption(resetsAt: Date().addingTimeInterval(-60), percent: 100)
        ViewHarness.render(caption)
        XCTAssertTrue(ViewTree(caption).shows("Limit reached · Reset time passed"))
    }

    func testNoCaptionWithoutResetOrText() {
        let caption = UsageLimitCaption(resetsAt: nil, percent: 20)
        XCTAssertEqual(ViewHarness.render(caption).height, 0)
        XCTAssertEqual(ViewTree(caption).texts, [])
    }
}
