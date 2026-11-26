@testable import TokenMon
import XCTest

/// The live OpenCode console client over a stub transport: request headers,
/// the org-scoped workspace walk, expired-session and server-error mapping,
/// and the account email lookup.
final class OpenCodeConsoleLiveTests: XCTestCase {
    private let meters = #"{"access":{"meters":{"month":{"usedMicroCents":1500000000,"limitMicroCents":6000000000}}}}"#

    private func client() -> OpenCodeConsoleClient {
        OpenCodeConsoleClient(cookieHeader: "auth=a; __Host-console_session=c", session: HTTPStub.session())
    }

    func testGoStatusRequestCarriesCookieAndWorkspaceHeaders() async throws {
        HTTPStub.reset(always: .json(meters))
        let (snapshot, org) = try await client().fetchGoUsageSnapshot(knownOrgID: "wrk_known")
        XCTAssertEqual(org, "wrk_known")
        XCTAssertEqual(snapshot.monthlyUsedPercent, 25, accuracy: 0.001)
        let request = try XCTUnwrap(HTTPStub.requests.first).request
        XCTAssertEqual(request.url?.absoluteString, "https://opencode.ai/console/api/go/status")
        XCTAssertEqual(request.httpMethod, "GET")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Cookie"), "auth=a; __Host-console_session=c")
        XCTAssertEqual(request.value(forHTTPHeaderField: "x-org-id"), "wrk_known")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "application/json")
        XCTAssertEqual(request.timeoutInterval, 20)
    }

    func testForbiddenWorkspaceIsSkippedForTheNextListedOne() async throws {
        HTTPStub.reset { request in
            switch (request.url?.path, request.value(forHTTPHeaderField: "x-org-id")) {
            case ("/console/api/orgs", nil): return .success(.json(#"[{"id":"wrk_a"},{"id":"wrk_b"},{"id":"usr_x"}]"#))
            case ("/console/api/go/status", "wrk_a"): return .success(.json("{}", status: 403))
            case ("/console/api/go/status", "wrk_b"): return .success(.json(self.meters))
            default: return .success(.json("{}", status: 404))
            }
        }
        let (_, org) = try await client().fetchGoUsageSnapshot()
        XCTAssertEqual(org, "wrk_b")
        XCTAssertNil(HTTPStub.requests.first?.request.value(forHTTPHeaderField: "x-org-id"), "listing orgs is not org-scoped")
    }

    func testRefusedSessionIsUnauthorized() async {
        HTTPStub.reset(always: .json("{}", status: 401))
        do {
            _ = try await client().fetchGoUsageSnapshot()
            XCTFail("expected unauthorized")
        } catch {
            XCTAssertEqual(error as? ProviderError, .unauthorized(.openCode))
        }
    }

    func testServerErrorAndChallengeAreTransient() throws {
        let url = try XCTUnwrap(URL(string: "https://opencode.ai/console/api/orgs"))
        let serverError = try XCTUnwrap(HTTPURLResponse(url: url, statusCode: 502, httpVersion: nil, headerFields: nil))
        XCTAssertThrowsError(try OpenCodeConsoleClient.check(serverError, data: Data("<b>bad</b>".utf8), orgID: nil)) {
            XCTAssertEqual($0 as? ProviderError, .badResponse(.openCode, "HTTP 502"))
        }
        let challenge = try XCTUnwrap(HTTPURLResponse(url: url, statusCode: 403, httpVersion: nil, headerFields: ["cf-mitigated": "challenge"]))
        XCTAssertThrowsError(try OpenCodeConsoleClient.check(challenge, data: Data(), orgID: nil)) {
            XCTAssertEqual($0 as? ProviderError, .badResponse(.openCode, ProviderHTTP.botChallengeMessage(status: 403)))
        }
        let ok = try XCTUnwrap(HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil))
        XCTAssertNoThrow(try OpenCodeConsoleClient.check(ok, data: Data(), orgID: nil))
    }

    func testAccountEmailComesFromTheConsoleSession() async {
        HTTPStub.reset(always: .json(#"{"user":{"email":"me@example.com"}}"#))
        let email = await client().fetchAccountEmail()
        XCTAssertEqual(email, "me@example.com")
        XCTAssertEqual(HTTPStub.requests.first?.request.url?.path, "/console/auth/session")

        HTTPStub.reset(always: .json("{}", status: 500))
        let none = await client().fetchAccountEmail()
        XCTAssertNil(none, "a failed lookup is best-effort")
    }

    func testNoWorkspaceAtAllIsReported() async {
        HTTPStub.reset(always: .json("[]"))
        do {
            _ = try await client().fetchGoUsageSnapshot()
            XCTFail("expected an error")
        } catch {
            XCTAssertEqual(error as? ProviderError, .badResponse(.openCode, "No OpenCode workspace on this account."))
        }
        do {
            _ = try await client().resolveOrgID()
            XCTFail("expected an error")
        } catch {
            XCTAssertEqual(error as? ProviderError, .badResponse(.openCode, "No OpenCode workspace on this account."))
        }
    }

    func testUnreadableBodiesAreBadResponses() {
        XCTAssertThrowsError(try OpenCodeConsoleClient.parseGoMeters(Data("[]".utf8)))
        XCTAssertThrowsError(try OpenCodeConsoleClient.parseOrgs(Data("{}".utf8)))
        XCTAssertNil(OpenCodeConsoleClient.usd(["not": "a number"]))
        XCTAssertEqual(OpenCodeConsoleClient.usd("250000000") ?? -1, 2.5, accuracy: 0.0001)
    }
}
