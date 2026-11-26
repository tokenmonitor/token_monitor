@testable import TokenMon
import WebKit
import XCTest

/// Reading, capturing and clearing cookies in an isolated, non-persistent
/// WebKit store. Nothing loads a page or touches the default store.
@MainActor
final class WebKitCookieStoreTests: XCTestCase {
    private func cookie(_ name: String, _ value: String, domain: String) -> HTTPCookie {
        HTTPCookie(properties: [.domain: domain, .path: "/", .name: name, .value: value])!
    }

    private func makeStore(_ cookies: [HTTPCookie]) async -> WKWebsiteDataStore {
        let store = WKWebsiteDataStore.nonPersistent()
        for cookie in cookies {
            await store.httpCookieStore.setCookie(cookie)
        }
        return store
    }

    private func names(in store: WKWebsiteDataStore) async -> Set<String> {
        await Set(WKWebsiteDataStoreBridge.shared.allCookies(in: store).map(\.name))
    }

    func testBridgeReadsEveryCookie() async {
        let store = await makeStore([
            cookie("sid", "1", domain: "example.com"),
            cookie("other", "2", domain: "elsewhere.test")
        ])
        let found = await names(in: store)
        XCTAssertEqual(found, ["sid", "other"])
    }

    func testClearCookiesRemovesOnlyMatchingDomains() async {
        let store = await makeStore([
            cookie("sid", "1", domain: "example.com"),
            cookie("other", "2", domain: "elsewhere.test")
        ])
        await WKWebsiteDataStoreBridge.shared.clearCookies(matching: { $0.contains("example.com") }, in: store)
        let found = await names(in: store)
        XCTAssertEqual(found, ["other"])
    }

    func testClearAllCookiesEmptiesTheStore() async {
        let store = await makeStore([
            cookie("sid", "1", domain: "example.com"),
            cookie("other", "2", domain: "elsewhere.test")
        ])
        await WKWebsiteDataStoreBridge.shared.clearAllCookies(in: store)
        let found = await names(in: store)
        XCTAssertTrue(found.isEmpty)
    }

    // MARK: - Capture

    private func policy(maxAttempts: Int = 1) -> WebKitCookieCapture.Policy {
        WebKitCookieCapture.Policy(
            isDomain: { $0.hasSuffix("example.com") },
            isPreferredSessionCookie: { $0.name == "sid" },
            looksLikeAuthCookie: { _ in false },
            includeAllDomainCookiesWhenSessionFound: false,
            maxAttempts: maxAttempts,
            retryDelayNanoseconds: 1_000_000,
            failureMessage: "no session"
        )
    }

    func testCaptureBuildsTheHeaderAndEmail() async throws {
        let store = await makeStore([
            cookie("sid", "abc", domain: "example.com"),
            cookie("user_email", "a%40b.com", domain: "example.com"),
            cookie("sid", "elsewhere", domain: "elsewhere.test")
        ])
        let captured = await WebKitCookieCapture.capture(policy: policy(), dataStore: store)
        let result = try XCTUnwrap(captured)
        XCTAssertEqual(result.cookieHeader, "sid=abc")
        XCTAssertEqual(result.cookies.map(\.domain), ["example.com"])
        XCTAssertNil(result.email, "only the session cookie is kept, so no email cookie is read")
    }

    /// An empty jar is retried `maxAttempts` times, then reported as no capture.
    func testCaptureRetriesThenGivesUp() async {
        let store = await makeStore([cookie("tracking", "1", domain: "example.com")])
        let start = Date()
        let captured = await WebKitCookieCapture.capture(policy: policy(maxAttempts: 3), dataStore: store)
        XCTAssertNil(captured)
        XCTAssertGreaterThanOrEqual(Date().timeIntervalSince(start), 0.002, "two retry waits ran")
    }

    /// Without a session cookie, auth-looking cookies qualify: the whole domain
    /// jar when the policy keeps it, else just those cookies.
    func testAuthLookingCookiesStandInForTheSession() {
        let jar = [
            cookie("auth_token", "t", domain: "example.com"),
            cookie("theme", "dark", domain: "example.com")
        ]
        // No preferred session cookie: the default never matches one.
        var narrow = WebKitCookieCapture.Policy(
            isDomain: { $0.hasSuffix("example.com") },
            looksLikeAuthCookie: { $0.name.hasPrefix("auth") },
            includeAllDomainCookiesWhenSessionFound: false,
            failureMessage: "no session"
        )
        XCTAssertEqual(WebKitCookieCapture.select(from: jar, policy: narrow)?.map(\.name), ["auth_token"])

        var wide = narrow
        wide.includeAllDomainCookiesWhenSessionFound = true
        XCTAssertEqual(WebKitCookieCapture.select(from: jar, policy: wide)?.map(\.name), ["auth_token", "theme"])

        // Neither a session nor an auth-looking cookie (the default matches none).
        XCTAssertNil(WebKitCookieCapture.select(from: jar, policy: WebKitCookieCapture.Policy(isDomain: { _ in true }, failureMessage: "none")))
    }

    // MARK: - Session capture

    func testSessionCaptureSignsIn() async {
        let auth = ClaudeAuthSession(store: InMemoryCredentialStore())
        await auth.signInDataStore.httpCookieStore.setCookie(cookie("sessionKey", "sk-live", domain: "claude.ai"))
        await auth.signInDataStore.httpCookieStore.setCookie(cookie("lastActiveOrg", "org-1", domain: "claude.ai"))

        let captured = await auth.captureCookiesFromWebKit()
        XCTAssertTrue(captured)
        XCTAssertTrue(auth.isSignedIn)
        XCTAssertNil(auth.lastAuthError)
    }

    func testSessionCaptureWithoutCookiesExplains() async {
        let config = ProviderAuthConfig(
            storeFilenamePrefix: "auth_",
            logCategory: "TestAuth",
            extraStoreKeys: [],
            signOutHosts: [],
            capturePolicy: policy(),
            isDomain: { $0.hasSuffix("example.com") }
        )
        let auth = ProviderAuthSession(config: config, store: InMemoryCredentialStore())
        let captured = await auth.captureCookiesFromWebKit()
        XCTAssertFalse(captured)
        XCTAssertFalse(auth.isSignedIn)
        XCTAssertEqual(auth.lastAuthError, "no session")
    }

    /// Refuses writes to one key, as a store whose label file cannot be written.
    private final class RefusingStore: CredentialStore {
        let refusedKey: String
        var values: [String: String] = [:]
        init(refusing key: String) { refusedKey = key }
        func value(forKey key: String) -> String? { values[key] }
        func set(_ value: String, forKey key: String) throws {
            guard key != refusedKey else { throw CredentialStoreError.fileWriteFailed }
            values[key] = value
        }

        func remove(forKey key: String) { values.removeValue(forKey: key) }
    }

    /// A label that cannot be saved does not fail the sign-in; the email still
    /// shows for this session.
    func testUnsavedEmailStillSignsIn() {
        let store = RefusingStore(refusing: "email")
        let auth = ClaudeAuthSession(store: store)
        let result = WebKitCookieCapture.CaptureResult(cookies: [], cookieHeader: "sessionKey=sk", email: "a@b.com")
        XCTAssertTrue(auth.adopt(result))
        XCTAssertTrue(auth.isSignedIn)
        XCTAssertEqual(auth.accountEmail, "a@b.com")
        XCTAssertEqual(store.values["session"], "sessionKey=sk")
        XCTAssertNil(store.values["email"])
    }
}
