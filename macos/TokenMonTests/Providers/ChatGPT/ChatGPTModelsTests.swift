@testable import TokenMon
import XCTest

/// ChatGPT snapshot and window values the panel and menu bar read: remaining
/// percent, headline, plan name, reset clock, and payload edge cases.
final class ChatGPTModelsTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    func testRemainingPercentIsTheClampedComplement() {
        XCTAssertEqual(ChatGPTUsageWindow(usedPercent: 30).remainingPercent, 70, accuracy: 0.001)
        XCTAssertEqual(ChatGPTUsageWindow(usedPercent: 120).remainingPercent, 0, accuracy: 0.001)
    }

    func testWindowLabelsFollowTheReportedLength() {
        XCTAssertEqual(ChatGPTUsageWindow(usedPercent: 0, windowSeconds: 604_800).label(fallback: "x"), "Weekly")
        XCTAssertEqual(ChatGPTUsageWindow(usedPercent: 0, windowSeconds: 86_400).label(fallback: "x"), "Daily")
        XCTAssertEqual(ChatGPTUsageWindow(usedPercent: 0, windowSeconds: 18_000).label(fallback: "x"), "5-Hour Window")
        XCTAssertEqual(ChatGPTUsageWindow(usedPercent: 0, windowSeconds: 172_800).label(fallback: "x"), "2-Day Window")
        XCTAssertEqual(ChatGPTUsageWindow(usedPercent: 0, windowSeconds: 5_400).label(fallback: "x"), "x")
        XCTAssertEqual(ChatGPTUsageWindow(usedPercent: 0).label(fallback: "Fallback"), "Fallback")
    }

    func testHeadlineAndResetComeFromPrimaryThenSecondary() {
        let weekly = ChatGPTUsageWindow(usedPercent: 55, resetsAt: now)
        var snapshot = ChatGPTSnapshot(fetchedAt: now, allowed: true, limitReached: false, primary: nil, secondary: weekly)
        XCTAssertEqual(snapshot.id, now)
        XCTAssertEqual(snapshot.headlineUsedPercent, 55, accuracy: 0.001)
        XCTAssertEqual(snapshot.resetsAt, now)
        snapshot.primary = ChatGPTUsageWindow(usedPercent: 10)
        XCTAssertEqual(snapshot.headlineUsedPercent, 10, accuracy: 0.001)
        XCTAssertNil(snapshot.resetsAt)
        snapshot.primary = nil
        snapshot.secondary = nil
        XCTAssertEqual(snapshot.headlineUsedPercent, 0, accuracy: 0.001)
    }

    func testDisplayPlanNameCapitalisesOrFallsBack() {
        var snapshot = ChatGPTSnapshot(fetchedAt: now, allowed: true, limitReached: false, primary: nil, secondary: nil)
        XCTAssertEqual(snapshot.displayPlanName, "ChatGPT")
        snapshot.planName = ""
        XCTAssertEqual(snapshot.displayPlanName, "ChatGPT")
        snapshot.planName = "PLUS"
        XCTAssertEqual(snapshot.displayPlanName, "Plus")
    }

    func testUnattributedLimitNeedsNoFullWindow() {
        let snapshot = ChatGPTSnapshot(
            fetchedAt: now,
            allowed: false,
            limitReached: true,
            primary: nil,
            secondary: nil
        )
        XCTAssertTrue(snapshot.hasUnattributedLimit)
        XCTAssertEqual(snapshot.usagePool, .mixed, "no window reads as mixed")
    }

    func testParseReadsISOResetDatesAndRejectsNonObjects() throws {
        let data = Data(#"{"five_hour":{"utilization":12,"reset_at":"2026-01-01T00:00:00Z"},"weekly":{"used_percent":3,"reset_at":[1]}}"#.utf8)
        let response = try ChatGPTUsageResponse.parse(data)
        XCTAssertEqual(response.primary?.resetsAt, ISO8601DateFormatter.parseFlexible("2026-01-01T00:00:00Z"))
        XCTAssertNil(response.secondary?.resetsAt, "a reset that is neither a number nor a string is ignored")
        XCTAssertThrowsError(try ChatGPTUsageResponse.parse(Data("[1]".utf8)))
    }

    func testAccountIDFallsBackToAnyAccountIDClaim() {
        func jwt(_ payload: String) -> String {
            "e30." + Data(payload.utf8).base64EncodedString().replacingOccurrences(of: "=", with: "") + ".sig"
        }
        XCTAssertEqual(ChatGPTAccountID.fromAccessToken(jwt(#"{"https://api.openai.com/auth":{"workspace_account_id":"w-9"}}"#)), "w-9")
        XCTAssertNil(ChatGPTAccountID.fromAccessToken(jwt(#"{"https://api.openai.com/auth":{"user":"u"}}"#)))
        XCTAssertNil(ChatGPTAccountID.fromAccessToken(jwt(#"{"sub":"u"}"#)))
        XCTAssertNil(ChatGPTAccountID.fromAccessToken("not-a-jwt"))
    }
}
