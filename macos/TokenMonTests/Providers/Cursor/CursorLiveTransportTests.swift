@testable import TokenMon
import XCTest

/// The live Cursor client over a stub transport: the dashboard GETs and the
/// usage-events POST carry the session cookie, events page until a short
/// page, the reported total, or the page cap, and an `/auth/me` failure
/// leaves the email out without failing the fetch.
final class CursorLiveTransportTests: XCTestCase {
    private let summary = #"{"individualUsage":{"plan":{"used":150,"limit":2000,"totalPercentUsed":7.5}}}"#

    private func eventsPage(count: Int, total: Int) -> String {
        let stamp = Int64(Date().addingTimeInterval(-3_600).timeIntervalSince1970 * 1000)
        let events = Array(repeating: #"{"timestamp":"\#(stamp)","chargedCents":1}"#, count: count)
        return #"{"totalUsageEventsCount":\#(total),"usageEventsDisplay":[\#(events.joined(separator: ","))]}"#
    }

    private func route(events: String, me: HTTPStub.Reply = .json(#"{"email":"me@example.com"}"#)) {
        HTTPStub.reset { request in
            switch request.url?.path {
            case "/api/usage-summary": return .success(.json(self.summary))
            case "/api/auth/me": return .success(me)
            case "/api/dashboard/get-filtered-usage-events": return .success(.json(events))
            default: return .success(.json("{}", status: 404))
            }
        }
    }

    func testFetchSendsTheCookieToEachDashboardEndpoint() async throws {
        route(events: eventsPage(count: 1, total: 1))
        let client = CursorUsageClient(cookieHeader: "WorkosCursorSessionToken=t", session: HTTPStub.session())

        let (snapshot, _, _) = try await client.fetchSnapshot()

        XCTAssertEqual(snapshot.usedPercent, 7.5, accuracy: 0.001)
        XCTAssertEqual(snapshot.accountEmail, "me@example.com")
        XCTAssertEqual(snapshot.costStats?.meteredCycleUSD ?? -1, 0.01, accuracy: 0.0001)
        let requests = HTTPStub.requests
        XCTAssertEqual(Set(requests.compactMap { $0.request.url?.path }), [
            "/api/usage-summary", "/api/auth/me", "/api/dashboard/get-filtered-usage-events"
        ])
        for sent in requests {
            XCTAssertEqual(sent.request.value(forHTTPHeaderField: "Cookie"), "WorkosCursorSessionToken=t")
            XCTAssertEqual(sent.request.value(forHTTPHeaderField: "Referer"), "https://cursor.com")
        }
        let post = try XCTUnwrap(requests.first { $0.request.httpMethod == "POST" })
        XCTAssertEqual(post.request.value(forHTTPHeaderField: "Origin"), "https://cursor.com")
        XCTAssertEqual(post.json?["page"] as? Int, 1)
        XCTAssertEqual(post.json?["pageSize"] as? Int, 500)
        XCTAssertNotNil(Int64(post.json?["startDate"] as? String ?? ""))
        XCTAssertNotNil(Int64(post.json?["endDate"] as? String ?? ""))
    }

    func testFullPagesContinueUntilTheReportedTotal() async throws {
        route(events: eventsPage(count: 500, total: 1_000))
        let client = CursorUsageClient(cookieHeader: "c=1", session: HTTPStub.session())
        _ = try await client.fetchSnapshot()
        let pages = HTTPStub.requests.compactMap { $0.json?["page"] as? Int }
        XCTAssertEqual(pages, [1, 2], "the total stops paging after two full pages")
    }

    func testPagingStopsAtThePageCapWithoutAKnownTotal() async throws {
        route(events: eventsPage(count: 500, total: 0))
        let client = CursorUsageClient(cookieHeader: "c=1", session: HTTPStub.session())
        _ = try await client.fetchSnapshot()
        let pages = HTTPStub.requests.compactMap { $0.json?["page"] as? Int }
        XCTAssertEqual(pages, Array(1...40))
    }

    func testFailedEmailLookupKeepsTheSnapshot() async throws {
        route(events: eventsPage(count: 0, total: 0), me: .json("{}", status: 500))
        let client = CursorUsageClient(cookieHeader: "c=1", session: HTTPStub.session())
        let (snapshot, _, _) = try await client.fetchSnapshot()
        XCTAssertNil(snapshot.accountEmail)
        XCTAssertEqual(snapshot.usedPercent, 7.5, accuracy: 0.001)
    }

    func testUnauthorizedSummaryFailsTheFetch() async {
        HTTPStub.reset(always: .json(#"{"error":"not_authenticated"}"#))
        let client = CursorUsageClient(cookieHeader: "c=1", session: HTTPStub.session())
        do {
            _ = try await client.fetchSnapshot()
            XCTFail("expected unauthorized")
        } catch {
            XCTAssertEqual(error as? ProviderError, .unauthorized(.cursor))
        }
    }
}
