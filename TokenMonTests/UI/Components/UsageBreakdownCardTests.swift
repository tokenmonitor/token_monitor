@testable import TokenMon
import XCTest

/// Legend percents for the Usage Breakdown card, as shares of the usage bar.
final class UsageBreakdownCardTests: XCTestCase {
    /// SuperGrok products at 25 / 12 / 6 / 4 % of the pool (47% used) stay
    /// percents of that bar. They are not stretched to fill 100%.
    func testPercentsStaySharesOfThePool() {
        let percents = UsageBreakdownCard.poolPercents([25, 12, 6, 4])
        XCTAssertEqual(percents, [25, 12, 6, 4])
        XCTAssertEqual(percents.reduce(0, +), 47)
    }

    /// Rounding each slice alone can miss the consumed total. The largest
    /// remainders take the missing points so the legend matches that total.
    func testPercentsSumToTheConsumedTotal() {
        let thirds = UsageBreakdownCard.poolPercents([1, 1, 1])
        XCTAssertEqual(thirds.reduce(0, +), 3)
        XCTAssertEqual(thirds, [1, 1, 1])
        let full = UsageBreakdownCard.poolPercents([50.31, 28.09, 21.6])
        XCTAssertEqual(full.reduce(0, +), 100)
        XCTAssertEqual(full, [50, 28, 22])
        let drifted = UsageBreakdownCard.poolPercents([10.4, 10.4, 10.2])
        XCTAssertEqual(drifted.reduce(0, +), 31)
    }

    func testNothingUsedGivesZeroPercents() {
        XCTAssertEqual(UsageBreakdownCard.poolPercents([0, 0]), [0, 0])
        XCTAssertEqual(UsageBreakdownCard.poolPercents([]), [])
        XCTAssertEqual(UsageBreakdownCard.poolPercents([-5, 10]), [0, 10])
    }

    /// Dollar slices fill the usage bar's consumed percent, split by spend.
    func testDollarSlicesFillTheConsumedPercent() {
        let dollars = UsageBreakdownCard.segmentPercents([2, 1], poolPercents: nil, consumedPercent: 70)
        XCTAssertEqual(dollars[0], 140.0 / 3, accuracy: 0.001)
        XCTAssertEqual(dollars[1], 70.0 / 3, accuracy: 0.001)
        let pool = UsageBreakdownCard.segmentPercents([25, 12], poolPercents: [25, 12], consumedPercent: 100)
        XCTAssertEqual(pool, [25, 12])
    }
}
