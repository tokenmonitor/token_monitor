@testable import TokenMon
import XCTest

final class SavedResetsTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    func testCountOutsideRangeIsNil() {
        XCTAssertNil(SavedResets(count: 0))
        XCTAssertNil(SavedResets(count: -1))
        XCTAssertNil(SavedResets(count: SavedResets.maximumCount + 1))
        XCTAssertEqual(SavedResets(count: SavedResets.maximumCount)?.count, SavedResets.maximumCount)
    }

    func testExpirationsDropExpiredAndKeepSoonest() throws {
        let soon = now.addingTimeInterval(3_600)
        let later = now.addingTimeInterval(86_400)
        let resets = try XCTUnwrap(SavedResets(
            expirations: [later, now.addingTimeInterval(-1), nil, soon, now],
            now: now
        ))
        XCTAssertEqual(resets.count, 3)
        XCTAssertEqual(resets.nextExpiry, soon)
    }

    func testExpirationsWithoutDatesHaveNoNextExpiry() throws {
        let resets = try XCTUnwrap(SavedResets(expirations: [nil, nil], now: now))
        XCTAssertEqual(resets.count, 2)
        XCTAssertNil(resets.nextExpiry)
    }

    func testAllExpiredIsNil() {
        XCTAssertNil(SavedResets(expirations: [now.addingTimeInterval(-60)], now: now))
        XCTAssertNil(SavedResets(expirations: [], now: now))
    }

    func testCountLabel() {
        XCTAssertEqual(SavedResets(count: 1)?.countLabel, "1 available")
        XCTAssertEqual(SavedResets(count: 3)?.countLabel, "3 available")
    }
}
