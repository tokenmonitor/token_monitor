@testable import TokenMon
import XCTest

/// The live ChatGPT transport: the cookie exchanges for an access token at
/// `/api/auth/session`, which then authorizes `wham/usage` with the account
/// header, and every `Set-Cookie` comes back for the poller to fold in.
final class ChatGPTLiveTransportTests: XCTestCase {
    /// A JWT whose payload carries `chatgpt_account_id` = "acct-1".
    private let token = "e30." + Data(#"{"https://api.openai.com/auth":{"chatgpt_account_id":"acct-1"}}"#.utf8)
        .base64EncodedString()
        .replacingOccurrences(of: "=", with: "") + ".sig"

    private let usage = #"{"plan_type":"pro","rate_limit":{"primary_window":{"used_percent":30}}}"#

    func testFetchExchangesTheCookieThenSendsBearerAndAccountHeader() async throws {
        HTTPStub.reset { request in
            switch request.url?.path {
            case "/api/auth/session":
                return .success(.json(#"{"accessToken":"\#(self.token)"}"#, headers: ["Set-Cookie": "renewed=1"]))
            case "/backend-api/wham/usage":
                return .success(.json(self.usage, headers: ["Set-Cookie": "usage=2"]))
            default:
                return .success(.json("{}", status: 404))
            }
        }
        let client = ChatGPTUsageClient(
            cookieHeader: "__Secure-next-auth.session-token=abc",
            transport: .live(through: HTTPStub.session())
        )

        let fetch = try await client.fetchUsage()

        XCTAssertEqual(fetch.response.planName, "pro")
        XCTAssertEqual(fetch.response.primary?.usedPercent ?? -1, 30, accuracy: 0.001)
        XCTAssertEqual(fetch.setCookieHeaders, ["renewed=1", "usage=2"])
        let requests = HTTPStub.requests.map(\.request)
        XCTAssertEqual(requests.map { $0.url?.path }, ["/api/auth/session", "/backend-api/wham/usage"])
        XCTAssertEqual(requests[0].value(forHTTPHeaderField: "Cookie"), "__Secure-next-auth.session-token=abc")
        XCTAssertNil(requests[0].value(forHTTPHeaderField: "Authorization"))
        XCTAssertEqual(requests[1].value(forHTTPHeaderField: "Authorization"), "Bearer \(token)")
        XCTAssertEqual(requests[1].value(forHTTPHeaderField: "ChatGPT-Account-Id"), "acct-1")
    }

    func testTokenWithoutAccountClaimSendsNoAccountHeader() async throws {
        HTTPStub.reset { request in
            request.url?.path == "/api/auth/session"
                ? .success(.json(#"{"accessToken":"opaque"}"#))
                : .success(.json(self.usage))
        }
        let client = ChatGPTUsageClient(cookieHeader: "c=1", transport: .live(through: HTTPStub.session()))
        _ = try await client.fetchUsage()
        XCTAssertNil(HTTPStub.requests.last?.request.value(forHTTPHeaderField: "ChatGPT-Account-Id"))
    }

    func testRejectedSessionExchangeIsUnauthorized() async {
        HTTPStub.reset(always: .json("{}", status: 401))
        let client = ChatGPTUsageClient(cookieHeader: "c=1", transport: .live(through: HTTPStub.session()))
        do {
            _ = try await client.fetchUsage()
            XCTFail("expected unauthorized")
        } catch {
            XCTAssertEqual(error as? ProviderError, .unauthorized(.chatGPT))
        }
        XCTAssertEqual(HTTPStub.requests.count, 1, "usage is never requested without a token")
    }
}
