@testable import TokenMon
import XCTest

/// `ProviderHTTP` builds each provider request (path, method, headers, JSON
/// body) and maps the response through the shared error contract.
final class ProviderHTTPTests: XCTestCase {
    private let base = URL(string: "https://api.example.com/v1")!

    override func setUp() {
        super.setUp()
        HTTPStub.reset(always: .json("{}"))
    }

    func testGetSendsCredentialsAndHeadersToTheResolvedPath() async throws {
        HTTPStub.reset(always: .json(#"{"ok":true}"#))
        let data = try await ProviderHTTP.get(
            "usage",
            baseURL: base,
            context: .cursor,
            cookieHeader: "sid=1",
            bearerToken: "tok",
            referer: "https://example.com/",
            headers: ["X-Extra": "yes"],
            session: HTTPStub.session()
        )
        XCTAssertEqual(try ProviderHTTP.jsonObject(data, context: .cursor)["ok"] as? Bool, true)
        let request = try XCTUnwrap(HTTPStub.requests.first).request
        XCTAssertEqual(request.url?.absoluteString, "https://api.example.com/v1/usage")
        XCTAssertEqual(request.httpMethod, "GET")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Cookie"), "sid=1")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer tok")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Referer"), "https://example.com/")
        XCTAssertEqual(request.value(forHTTPHeaderField: "X-Extra"), "yes")
        XCTAssertNil(request.value(forHTTPHeaderField: "Origin"))
    }

    func testPostSendsJSONBodyAndOrigin() async throws {
        _ = try await ProviderHTTP.post(
            "/api/thing",
            baseURL: base,
            context: .cursor,
            json: ["teamId": 7],
            origin: "https://example.com",
            session: HTTPStub.session()
        )
        let sent = try XCTUnwrap(HTTPStub.requests.first)
        XCTAssertEqual(sent.request.url?.absoluteString, "https://api.example.com/api/thing")
        XCTAssertEqual(sent.request.httpMethod, "POST")
        XCTAssertEqual(sent.request.value(forHTTPHeaderField: "Content-Type"), "application/json")
        XCTAssertEqual(sent.request.value(forHTTPHeaderField: "Origin"), "https://example.com")
        XCTAssertEqual(sent.json?["teamId"] as? Int, 7)
    }

    func testGetWithResponseExposesSetCookieHeaders() async throws {
        HTTPStub.reset(always: .json("{}", headers: ["Set-Cookie": "sid=2; Path=/"]))
        let result = try await ProviderHTTP.getWithResponse(
            "me",
            baseURL: base,
            context: .chatGPT,
            session: HTTPStub.session()
        )
        XCTAssertEqual(result.response.statusCode, 200)
        XCTAssertEqual(ProviderHTTP.setCookieHeaders(from: result.response), ["sid=2; Path=/"])
    }

    func testUnauthorizedStatusMapsToTheProviderContext() async {
        HTTPStub.reset(always: .json("{}", status: 401))
        do {
            _ = try await ProviderHTTP.get("x", baseURL: base, context: .claude, session: HTTPStub.session())
            XCTFail("expected unauthorized")
        } catch {
            XCTAssertEqual(error as? ProviderError, .unauthorized(.claude))
        }
    }

    func testForbiddenMessageReplacesUnauthorizedForAPlain403() async {
        HTTPStub.reset(always: .json("{}", status: 403))
        let context = ProviderErrorContext.grokbot
        let message = try? XCTUnwrap(context.forbiddenMessage)
        do {
            _ = try await ProviderHTTP.post("x", baseURL: base, context: context, json: [:], session: HTTPStub.session())
            XCTFail("expected the forbidden message")
        } catch {
            XCTAssertEqual(error.localizedDescription, message)
        }
    }

    func testTransportFailureMapsToNetworkError() async {
        HTTPStub.reset { _ in .failure(URLError(.notConnectedToInternet)) }
        do {
            _ = try await ProviderHTTP.get("x", baseURL: base, context: .openRouter, session: HTTPStub.session())
            XCTFail("expected a network error")
        } catch {
            guard case .network(.openRouter, _) = error as? ProviderError else {
                return XCTFail("unexpected \(error)")
            }
        }
    }

    func testUnresolvablePathIsABadResponseAndSendsNothing() async {
        do {
            _ = try await ProviderHTTP.get("http://[", baseURL: base, context: .cursor, session: HTTPStub.session())
            XCTFail("expected a bad response")
        } catch {
            XCTAssertEqual(error as? ProviderError, .badResponse(.cursor, "Invalid path http://["))
        }
        XCTAssertTrue(HTTPStub.requests.isEmpty)
    }

    func testUnauthorizedBodyMessagesAreRecognised() {
        XCTAssertTrue(ProviderHTTP.isUnauthorizedMessage("NOT_AUTHENTICATED"))
        XCTAssertTrue(ProviderHTTP.isUnauthorizedMessage("Unauthorized request"))
        XCTAssertFalse(ProviderHTTP.isUnauthorizedMessage("rate limited"))
    }

    func testLoginPageDetectionSkipsChallengesAndNonHTML() {
        XCTAssertTrue(ProviderHTTP.looksLikeLoginPage(Data("<html><form><input type=\"password\">".utf8)))
        XCTAssertFalse(ProviderHTTP.looksLikeLoginPage(Data("<html><title>Just a moment...</title> login".utf8)))
        XCTAssertFalse(ProviderHTTP.looksLikeLoginPage(Data(#"{"login":true}"#.utf8)))
        XCTAssertFalse(ProviderHTTP.looksLikeHTML(Data([0xFF, 0xFE, 0xFD])))
    }
}
