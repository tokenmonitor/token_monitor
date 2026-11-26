import SwiftUI
@testable import TokenMon
import XCTest

/// Segment-width math for the weekly pool bar.
final class SegmentedUsageBarTests: XCTestCase {
    func testWidthsAreProportionalWhenTheyFit() {
        let widths = SegmentedUsageBar.segmentWidths(percents: [50, 25, 25], usable: 100, minWidth: 3)
        XCTAssertEqual(widths, [50, 25, 25])
    }

    /// Several tiny slices each floored at the minimum are normalized down so
    /// the row fits the track.
    func testFloorsAreNormalizedSoTheyDoNotOverflow() {
        let widths = SegmentedUsageBar.segmentWidths(percents: [0.1, 0.1, 0.1], usable: 5, minWidth: 3)
        XCTAssertEqual(widths.reduce(0, +), 5, accuracy: 0.0001)
    }

    func testZeroUsableKeepsFloors() {
        let widths = SegmentedUsageBar.segmentWidths(percents: [50, 50], usable: 0, minWidth: 3)
        XCTAssertEqual(widths, [3, 3])
    }

    /// A part-used pool draws its slices and the remaining track; a full
    /// one only slices, and an empty one only the track. Each is one bar tall.
    @MainActor
    func testBarDrawsSlicesAndTheRemainingTrack() {
        let partial = SegmentedUsageBar(segments: [
            .init(id: "chat", percent: 30, color: .blue),
            .init(id: "tiny", percent: 0.01, color: .red),
            .init(id: "build", percent: 12, color: .green)
        ])
        let full = SegmentedUsageBar(segments: [.init(id: "chat", percent: 100, color: .blue)], height: 6)
        let empty = SegmentedUsageBar(segments: [])
        XCTAssertEqual(ViewHarness.renderBothAppearances(partial).height, 8)
        XCTAssertEqual(ViewHarness.render(full).height, 6)
        XCTAssertEqual(ViewHarness.render(empty).height, 8)
        XCTAssertNotEqual(ViewHarness.pixels(partial), ViewHarness.pixels(empty))
        XCTAssertNotEqual(ViewHarness.pixels(full), ViewHarness.pixels(empty))
    }

    func testProductColorsComeFromThePalette() throws {
        let resolved = try XCTUnwrap(NSColor(Color.product(.build)).usingColorSpace(.sRGB))
        XCTAssertEqual(resolved.blueComponent, ProductColor.build.sRGB.blue, accuracy: 0.01)
    }
}
