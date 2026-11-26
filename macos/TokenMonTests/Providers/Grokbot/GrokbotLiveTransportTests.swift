@testable import TokenMon
import XCTest

/// The live Grokbot client over a stub transport: the usage-status POST
/// authenticates with the Cursor cookie, the breakdown RPC with the access
/// token inside it, and a cookie without a token skips the breakdown.
final class GrokbotLiveTransportTests: XCTestCase {
    /// A three-part JWT inside a `WorkosCursorSessionToken` cookie.
    private let jwt = "aaa.bbb.ccc"
    private var cookie: String { "WorkosCursorSessionToken=user_1%3A%3A\(jwt)" }

    func testUsageStatusPostsWithTheCookie() async throws {
        HTTPStub.reset(always: .json(#"{"usagePercent":25,"nextResetTimestampUtc":"2099-01-01T00:00:00Z"}"#))
        let client = GrokbotUsageClient(cookieHeader: cookie, accountEmail: "me@example.com", session: HTTPStub.session())
        let snapshot = try await client.fetchSnapshot()
        XCTAssertEqual(snapshot.accountEmail, "me@example.com")
        let sent = try XCTUnwrap(HTTPStub.requests.first)
        XCTAssertEqual(sent.request.url?.absoluteString, "https://cursor.com/api/dashboard/get-sand-usage-status")
        XCTAssertEqual(sent.request.httpMethod, "POST")
        XCTAssertEqual(sent.request.value(forHTTPHeaderField: "Cookie"), cookie)
        XCTAssertEqual(sent.request.value(forHTTPHeaderField: "Referer"), "https://cursor.com/bot")
        XCTAssertEqual(sent.request.value(forHTTPHeaderField: "Origin"), "https://cursor.com")
        XCTAssertEqual(sent.json?.count, 0, "the body is an empty JSON object")
    }

    func testBreakdownUsesTheBearerTokenOnTheRPCHost() async throws {
        HTTPStub.reset(always: .json(#"{"weeklyLimitPpm":{"bySource":[{"source":"GROK_BOT_USAGE_SOURCE_USER","ppm":120000}]}}"#))
        let client = GrokbotUsageClient(cookieHeader: cookie, session: HTTPStub.session())
        let fetched = try await client.fetchBreakdown()
        let breakdown = try XCTUnwrap(fetched)
        XCTAssertEqual(breakdown.slices.first?.source, .user)
        XCTAssertEqual(breakdown.total, 12, accuracy: 0.001)
        let request = try XCTUnwrap(HTTPStub.requests.first).request
        XCTAssertEqual(request.url?.absoluteString, "https://api2.cursor.sh/aiserver.v1.DashboardService/GetGrokBotUsageBreakdown")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer \(jwt)")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Connect-Protocol-Version"), "1")
        XCTAssertNil(request.value(forHTTPHeaderField: "Cookie"))
    }

    func testCookieWithoutATokenSkipsTheBreakdown() async throws {
        HTTPStub.reset(always: .json("{}"))
        let client = GrokbotUsageClient(cookieHeader: "WorkosCursorSessionToken=opaque", session: HTTPStub.session())
        let breakdown = try await client.fetchBreakdown()
        XCTAssertNil(breakdown)
        XCTAssertTrue(HTTPStub.requests.isEmpty)
    }
}
