@testable import TokenMon
import XCTest

/// `usage-summary` fallbacks when the dashboard percent fields are missing,
/// the email and events-page edge cases, and the snapshot labels the panel
/// shows.
final class CursorSummaryParsingTests: XCTestCase {
    private func parse(_ json: String) throws -> CursorSnapshot {
        try CursorUsageClient.parseSummary(data: Data(json.utf8))
    }

    func testSinglePoolPercentStandsInForTheTotal() throws {
        XCTAssertEqual(try parse(#"{"individualUsage":{"plan":{"apiPercentUsed":12}}}"#).usedPercent, 12, accuracy: 0.001)
        let auto = try parse(#"{"individualUsage":{"plan":{"autoPercentUsed":30}}}"#)
        XCTAssertEqual(auto.usedPercent, 30, accuracy: 0.001)
        XCTAssertEqual(auto.pools.map(\.kind), [.total, .auto])
    }

    func testOverallThenPooledSpendDriveThePercentAndDollars() throws {
        let overall = try parse(#"{"individualUsage":{"overall":{"used":500,"limit":2000}}}"#)
        XCTAssertEqual(overall.usedPercent, 25, accuracy: 0.001)
        XCTAssertEqual(overall.planUsedUSD ?? -1, 5, accuracy: 0.001)
        XCTAssertEqual(overall.planLimitUSD ?? -1, 20, accuracy: 0.001)

        let pooled = try parse(#"{"teamUsage":{"pooled":{"used":300,"limit":1000}}}"#)
        XCTAssertEqual(pooled.usedPercent, 30, accuracy: 0.001)
        XCTAssertEqual(pooled.planUsedUSD ?? -1, 3, accuracy: 0.001)
        XCTAssertEqual(pooled.planLimitUSD ?? -1, 10, accuracy: 0.001)
    }

    func testEmptySummaryIsZeroWithoutPlanDollars() throws {
        let empty = try parse("{}")
        XCTAssertEqual(empty.usedPercent, 0)
        XCTAssertNil(empty.planUsedUSD)
        XCTAssertNil(empty.planLimitUSD)
        XCTAssertFalse(empty.onDemandEnabled)
        XCTAssertThrowsError(try parse("[]"))
    }

    func testOnDemandSpendIsReadInDollars() throws {
        let snapshot = try parse(#"{"individualUsage":{"onDemand":{"enabled":true,"used":250,"limit":10000}}}"#)
        XCTAssertTrue(snapshot.onDemandEnabled)
        XCTAssertEqual(snapshot.onDemandUsedUSD ?? -1, 2.5, accuracy: 0.001)
        XCTAssertEqual(snapshot.onDemandLimitUSD ?? -1, 100, accuracy: 0.001)
    }

    func testEmailNeedsAnAddressInAJSONObject() {
        XCTAssertEqual(CursorUsageClient.parseEmail(from: Data(#"{"email":"a@b.co"}"#.utf8)), "a@b.co")
        XCTAssertNil(CursorUsageClient.parseEmail(from: Data(#"{"email":"nobody"}"#.utf8)))
        XCTAssertNil(CursorUsageClient.parseEmail(from: Data("<html>".utf8)))
    }

    func testEventsPageMustBeAnObject() {
        XCTAssertThrowsError(try CursorUsageClient.parseUsageEventsPage(data: Data("[]".utf8)))
    }

    func testPoolLabelsAndRemaining() {
        XCTAssertEqual(CursorPoolKind.allCases.map(\.label), ["Total", "Auto", "API"])
        XCTAssertEqual(CursorPoolKind.allCases.map(\.trackLabel), ["Total", "Auto + Composer", "API (Other Models)"])
        let pool = CursorPoolUsage(kind: .api, usedPercent: 130)
        XCTAssertEqual(pool.id, .api)
        XCTAssertEqual(pool.remainingPercent, 0)
    }

    func testPlanNameIsPrefixedWithCursorOnce() throws {
        var snapshot = try parse("{}")
        XCTAssertEqual(snapshot.displayPlanName, "Cursor")
        snapshot.membershipType = "pro"
        XCTAssertEqual(snapshot.displayPlanName, "Cursor Pro")
        snapshot.membershipType = "CURSOR ULTRA"
        XCTAssertEqual(snapshot.displayPlanName, "Cursor ultra")
        XCTAssertEqual(snapshot.id, snapshot.fetchedAt)
        XCTAssertEqual(snapshot.usagePool, .monthly)
    }

    func testEventWeightPrefersRequestsThenTokensThenCents() throws {
        func weight(_ fields: String) throws -> Double {
            let event = try XCTUnwrap(CursorUsageEvent(json: try XCTUnwrap(
                JSONSerialization.jsonObject(with: Data(#"{"timestamp":"1775418973898"\#(fields)}"#.utf8)) as? [String: Any]
            )))
            return event.weight
        }
        XCTAssertEqual(try weight(#","requestsCosts":3,"chargedCents":9"#), 3)
        XCTAssertEqual(try weight(#","tokenUsage":{"inputTokens":4,"outputTokens":1},"chargedCents":9"#), 5)
        XCTAssertEqual(try weight(#","chargedCents":9"#), 9)
        XCTAssertEqual(try weight(""), 1)
    }
}
