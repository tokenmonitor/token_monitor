@testable import TokenMon
import XCTest

final class ClaudeUsageClientTests: XCTestCase {
    private let fixture = Data("""
    {
      "five_hour": {
        "utilization": 42.5,
        "resets_at": "2026-08-21T14:00:00.127279+00:00"
      },
      "seven_day": {
        "utilization": 87.0,
        "resets_at": "2026-08-27T11:00:00+00:00"
      }
    }
    """.utf8)

    func testParseWindows() throws {
        let response = try ClaudeUsageResponse.parse(fixture)
        XCTAssertEqual(response.fiveHour?.usedPercent ?? -1, 42.5, accuracy: 0.001)
        XCTAssertEqual(response.sevenDay?.usedPercent ?? -1, 87.0, accuracy: 0.001)
    }

    func testParseResetDates() throws {
        let response = try ClaudeUsageResponse.parse(fixture)
        let fiveHourReset = try XCTUnwrap(response.fiveHour?.resetsAt)
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        let components = calendar.dateComponents([.hour, .minute], from: fiveHourReset)
        XCTAssertEqual(components.hour, 14)
        XCTAssertEqual(components.minute, 0)

        // Weekly reset uses the plain (no-fraction) ISO-8601 form.
        let weeklyReset = try XCTUnwrap(response.sevenDay?.resetsAt)
        let weeklyComponents = calendar.dateComponents([.hour], from: weeklyReset)
        XCTAssertEqual(weeklyComponents.hour, 11)
    }

    func testParseMissingWindowsYieldsNil() throws {
        let data = Data("{}".utf8)
        let response = try ClaudeUsageResponse.parse(data)
        XCTAssertNil(response.fiveHour)
        XCTAssertNil(response.sevenDay)
    }

    func testParseClampsOutOfRangeUtilization() throws {
        let data = Data(#"{"five_hour": {"utilization": 130.0}}"#.utf8)
        let response = try ClaudeUsageResponse.parse(data)
        XCTAssertEqual(response.fiveHour?.usedPercent, 100)
    }

    /// An expired session 200s with an HTML sign-in page; that is an expired
    /// session, not a malformed payload.
    func testParseHTMLBodyIsUnauthorized() {
        let html = Data("<!doctype html><html><body>Sign in to Claude</body></html>".utf8)
        XCTAssertThrowsError(try ClaudeUsageResponse.parse(html)) { error in
            XCTAssertEqual(error as? ProviderError, .unauthorized(.claude))
        }
    }

    /// A malformed body is transient, so the poller keeps the last-good snapshot
    /// and the user stays signed in.
    func testParseMalformedBodyIsBadResponse() {
        XCTAssertThrowsError(try ClaudeUsageResponse.parse(Data("not json".utf8))) { error in
            guard let providerError = error as? ProviderError, case .badResponse = providerError else {
                return XCTFail("expected badResponse, got \(error)")
            }
        }
    }

    func testOrganizationIDFromCookieHeader() {
        let header = "__cf_bm=abc; lastActiveOrg=9d7a1b2c-1234-5678-90ab-cdef12345678; sessionKey=sk-ant-sid01-xyz"
        XCTAssertEqual(
            ClaudeUsageClient.organizationID(fromCookieHeader: header),
            "9d7a1b2c-1234-5678-90ab-cdef12345678"
        )
        XCTAssertNil(ClaudeUsageClient.organizationID(fromCookieHeader: "sessionKey=abc"))
        XCTAssertNil(ClaudeUsageClient.organizationID(fromCookieHeader: ""))
    }
    /// Live payload shape for accounts whose `seven_day` is null: the weekly pool
    /// is reported only in `limits`.
    private let limitsFixture = Data("""
    {
      "five_hour": {"utilization": 14.0, "resets_at": "2026-10-10T20:20:00.143736+00:00"},
      "seven_day": null,
      "seven_day_opus": null,
      "limits": [
        {"kind": "session", "group": "session", "percent": 14, "resets_at": "2026-10-10T20:20:00.143736+00:00",
         "scope": null, "is_active": true},
        {"kind": "weekly_scoped", "group": "weekly", "percent": 3, "resets_at": "2026-10-11T08:00:00+00:00",
         "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null}, "is_active": false}
      ]
    }
    """.utf8)

    func testParseFallsBackToWeeklyLimitWhenSevenDayIsNull() throws {
        let response = try ClaudeUsageResponse.parse(limitsFixture)
        let weekly = try XCTUnwrap(response.sevenDay)
        XCTAssertEqual(weekly.usedPercent, 3, accuracy: 0.001)
        XCTAssertEqual(weekly.scopeName, "Fable")
        XCTAssertEqual(weekly.resetsAt, ISO8601DateFormatter.parseFlexible("2026-10-11T08:00:00+00:00"))
        XCTAssertEqual(response.fiveHour?.usedPercent ?? -1, 14, accuracy: 0.001)
        let snapshot = ClaudeSnapshot(fetchedAt: Date(), fiveHour: response.fiveHour, sevenDay: weekly)
        XCTAssertEqual(snapshot.weeklyLabel, "Weekly · Fable")
    }

    func testParsePrefersAccountWideWeeklyLimit() throws {
        let data = Data("""
        {"limits": [
          {"group": "weekly", "percent": 80, "scope": {"model": {"display_name": "Fable"}}, "is_active": true},
          {"group": "weekly", "percent": 20, "scope": null, "is_active": false}
        ]}
        """.utf8)
        let weekly = try XCTUnwrap(ClaudeUsageResponse.parse(data).sevenDay)
        XCTAssertEqual(weekly.usedPercent, 20, accuracy: 0.001)
        XCTAssertNil(weekly.scopeName)
        XCTAssertNil(weekly.resetsAt)
    }

    func testParseSevenDayObjectWinsOverLimits() throws {
        let data = Data("""
        {"seven_day": {"utilization": 40}, "limits": [{"group": "weekly", "percent": 5}]}
        """.utf8)
        XCTAssertEqual(try ClaudeUsageResponse.parse(data).sevenDay?.usedPercent ?? -1, 40, accuracy: 0.001)
    }

    // MARK: - Saved resets (cedar_ember)

    private let resetNow = ISO8601DateFormatter().date(from: "2026-11-22T12:00:00Z")!

    private func savedResets(_ block: String) throws -> SavedResets? {
        try ClaudeUsageResponse.parse(Data(#"{"cedar_ember": \#(block)}"#.utf8), now: resetNow).savedResets
    }

    func testSavedResetsSumAvailableGrants() throws {
        let resets = try XCTUnwrap(savedResets("""
        {"eligible": true, "grants": [
          {"id": "g1", "resets_left": 1, "resets_total": 1, "paused": false,
           "starts_at": "2026-11-01T00:00:00Z", "ends_at": "2026-12-20T00:00:00Z"},
          {"id": "g2", "resets_left": 2, "resets_total": 2, "paused": false,
           "starts_at": null, "ends_at": "2026-12-05T00:00:00Z"},
          {"id": "g3", "resets_left": 1, "paused": false}
        ]}
        """))
        XCTAssertEqual(resets.count, 4)
        XCTAssertEqual(resets.nextExpiry, ISO8601DateFormatter().date(from: "2026-12-05T00:00:00Z"))
    }

    func testSavedResetsSkipUnavailableGrants() throws {
        XCTAssertNil(try savedResets("""
        {"eligible": true, "grants": [
          {"resets_left": 1, "paused": true},
          {"resets_left": 0, "resets_total": 1, "paused": false},
          {"resets_left": 1, "paused": false, "starts_at": "2026-12-01T00:00:00Z"},
          {"resets_left": 1, "paused": false, "ends_at": "2026-11-22T12:00:00Z"}
        ]}
        """))
    }

    /// A malformed grant is dropped; the well-formed one still counts.
    func testSavedResetsDropMalformedGrants() throws {
        let resets = try XCTUnwrap(savedResets("""
        {"eligible": true, "grants": [
          {"resets_left": "many", "paused": false},
          {"resets_left": 2, "resets_total": 1, "paused": false},
          {"resets_left": -1, "paused": false},
          {"resets_left": 1},
          {"resets_left": 1, "paused": null},
          {"resets_left": 1, "paused": false, "ends_at": "next tuesday"},
          {"resets_left": 1, "paused": false, "starts_at": 5},
          {"resets_left": 1, "resets_total": 1, "paused": false}
        ]}
        """))
        XCTAssertEqual(resets.count, 1)
        XCTAssertNil(resets.nextExpiry)
    }

    func testSavedResetsNeedEligibleBlock() throws {
        let grant = #"{"resets_left": 1, "paused": false}"#
        XCTAssertNil(try savedResets(#"{"eligible": false, "grants": [\#(grant)]}"#))
        XCTAssertNil(try savedResets(#"{"grants": [\#(grant)]}"#))
        XCTAssertNil(try savedResets(#"{"eligible": true}"#))
        XCTAssertNil(try savedResets("null"))
        XCTAssertNil(try ClaudeUsageResponse.parse(fixture).savedResets)
    }

    func testSavedResetsImplausibleInventoryShowsNothing() throws {
        XCTAssertNil(try savedResets("""
        {"eligible": true, "grants": [{"resets_left": 5000, "paused": false}]}
        """))
    }
}
