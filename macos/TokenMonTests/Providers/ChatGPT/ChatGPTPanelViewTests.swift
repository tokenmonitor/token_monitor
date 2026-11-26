import SwiftUI
@testable import TokenMon
import XCTest

/// The ChatGPT panel shows the Codex windows, the limit and blocked notes,
/// and the sign-in controls per state; its sign-in config finds chatgpt.com.
@MainActor
final class ChatGPTPanelViewTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var defaults: UserDefaults!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "ChatGPTPanel-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suiteName)
    }

    private func response(
        plan: String? = "plus",
        allowed: Bool = true,
        limitReached: Bool = false,
        primary: Double? = 35,
        secondary: Double? = 60,
        resets: SavedResets? = nil
    ) -> ChatGPTUsageResponse {
        ChatGPTUsageResponse(
            planName: plan,
            allowed: allowed,
            limitReached: limitReached,
            primary: primary.map { ChatGPTUsageWindow(usedPercent: $0, resetsAt: Date().addingTimeInterval(3600), windowSeconds: 5 * 3600) },
            secondary: secondary.map {
                ChatGPTUsageWindow(usedPercent: $0, resetsAt: Date().addingTimeInterval(3 * 86400), windowSeconds: ChatGPTSnapshot.weekSeconds)
            },
            savedResets: resets
        )
    }

    private func makeAuth(signedIn: Bool) -> ChatGPTAuthSession {
        let auth = ChatGPTAuthSession(directory: dir, store: InMemoryCredentialStore())
        if signedIn {
            auth.save(cookieHeader: "__Secure-next-auth.session-token=test")
        }
        return auth
    }

    private func makePoller(_ auth: ChatGPTAuthSession, fetch: @escaping () throws -> ChatGPTUsageResponse) -> ChatGPTUsagePoller {
        ChatGPTUsagePoller(
            settings: AppSettings(defaults: defaults),
            auth: auth,
            daily: DailyQuotaDeltaStore(
                store: FileBackedStringStore(directory: dir, filenamePrefix: "daily_"),
                storageKey: "chatgpt_daily_usage"
            ),
            fetchUsage: { _, _ in ChatGPTUsageClient.Fetch(response: try fetch(), fetchedAt: Date(), setCookieHeaders: []) }
        )
    }

    private func loaded(_ response: ChatGPTUsageResponse) async -> (ChatGPTUsagePoller, ChatGPTAuthSession) {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { response }
        await poller.refreshNow()
        XCTAssertNotNil(poller.snapshot)
        return (poller, auth)
    }

    func testSignedOutPanelOffersSignIn() {
        let auth = makeAuth(signedIn: false)
        let poller = makePoller(auth) { self.response() }
        var opened = 0
        let view = ChatGPTPanelView(poller: poller, auth: auth) { opened += 1 }
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Sign in to chatgpt.com to load your 5-hour and weekly Codex usage."))
        tree.press("Sign In to ChatGPT…")
        XCTAssertEqual(opened, 1)
    }

    func testWindowsPlanAndSavedResets() async {
        let (poller, auth) = await loaded(response(resets: SavedResets(count: 3, nextExpiry: nil)))
        let view = ChatGPTPanelView(poller: poller, auth: auth) {}
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Plus"))
        XCTAssertEqual(tree.all(SlimUsageTrack.self).map(\.label), ["5-Hour Window", "Weekly"])
        XCTAssertEqual(tree.all(SlimUsageTrack.self).map(\.percent), [35, 60])
        XCTAssertTrue(tree.shows("Resets available"))
        XCTAssertFalse(tree.shows("Limit reached · Window not reported"))
        XCTAssertFalse(tree.shows("Codex access is currently blocked"))
        XCTAssertFalse(tree.shows("Sign In Again…"))
    }

    func testLimitReachedWithoutWindowsAndBlockedAccess() async {
        let (poller, auth) = await loaded(response(plan: nil, allowed: false, limitReached: true, primary: nil, secondary: nil))
        let view = ChatGPTPanelView(poller: poller, auth: auth) {}
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("ChatGPT"))
        // Both tracks are always drawn, empty when their window is missing.
        XCTAssertEqual(tree.all(SlimUsageTrack.self).map(\.label), ["5-Hour Window", "Weekly"])
        XCTAssertEqual(tree.all(SlimUsageTrack.self).map(\.percent), [0, 0])
        XCTAssertTrue(tree.shows("Limit reached · Window not reported"))
        XCTAssertTrue(tree.shows("Codex access is currently blocked for this account."))
        XCTAssertFalse(tree.shows("Resets available"))
    }

    func testExpiredSessionKeepsDataAndAsksToSignInAgain() async {
        let auth = makeAuth(signedIn: true)
        var fail = false
        let poller = makePoller(auth) {
            if fail { throw URLError(.timedOut) }
            return self.response()
        }
        await poller.refreshNow()
        fail = true
        await poller.refreshNow()
        auth.markSessionInvalid(reason: "expired")
        var opened = 0
        let view = ChatGPTPanelView(poller: poller, auth: auth) { opened += 1 }
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertEqual(tree.first(StaleDataCaption.self)?.message, poller.lastError)
        tree.press("Sign In Again…")
        XCTAssertEqual(opened, 1)
    }

    func testSignOutClearsTheSession() async {
        let (poller, auth) = await loaded(response())
        ViewTree(ChatGPTPanelView(poller: poller, auth: auth) {}).first(ProviderSignOutButton.self)?.action()
        XCTAssertFalse(auth.isSignedIn)
        XCTAssertNil(poller.snapshot)
    }

    func testSignedInWithoutDataShowsStatus() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { throw URLError(.notConnectedToInternet) }
        let waiting = ChatGPTPanelView(poller: poller, auth: auth) {}
        ViewHarness.render(waiting)
        XCTAssertTrue(ViewTree(waiting).shows("No usage data yet."))

        await poller.refreshNow()
        let failed = ChatGPTPanelView(poller: poller, auth: auth) {}
        ViewHarness.render(failed)
        XCTAssertTrue(ViewTree(failed).shows(poller.lastError ?? "-"))
        XCTAssertFalse(ViewTree(failed).shows("Sign In"))
    }

    func testSignInAuthHostsMatchExactlyOrBySubdomainNotSubstring() {
        for host in ["auth.openai.com.evil.example", "evilaccounts.google.com", "notauth.openai.com",
                     "login.microsoft.evil.example", "login.microsoftonline.com.evil.example",
                     "appleid.apple.evil.example"] {
            XCTAssertFalse(ChatGPTAuthSession.isAuthHost(host: host, path: "/"), host)
        }
        // A login path counts only on chatgpt.com itself.
        XCTAssertFalse(ChatGPTAuthSession.isAuthHost(host: "evil.example", path: "/login"))
        XCTAssertFalse(ChatGPTAuthSession.isAuthHost(host: "chatgpt.com.evil.example", path: "/signin"))

        for host in ["AUTH.openai.com", "login.live.com", "login.microsoft.com", "idmsa.apple.com", "eu.auth0.openai.com"] {
            XCTAssertTrue(ChatGPTAuthSession.isAuthHost(host: host, path: "/"), host)
        }
        XCTAssertTrue(ChatGPTAuthSession.isAuthHost(host: "www.chatgpt.com", path: "/Auth/Login"))
        XCTAssertFalse(ChatGPTAuthSession.isReturnPage(URL(staticString: "https://chatgpt.com.evil.example/")))
    }

    func testSignInRecognizesAuthHostsAndTheReturnPage() throws {
        for host in ["auth.openai.com", "auth0.openai.com", "accounts.google.com", "appleid.apple.com", "login.microsoftonline.com"] {
            XCTAssertTrue(ChatGPTAuthSession.isAuthHost(host: host, path: "/"), host)
        }
        XCTAssertTrue(ChatGPTAuthSession.isAuthHost(host: "chatgpt.com", path: "/auth/login"))
        XCTAssertTrue(ChatGPTAuthSession.isAuthHost(host: "chatgpt.com", path: "/signin"))
        XCTAssertFalse(ChatGPTAuthSession.isAuthHost(host: "chatgpt.com", path: "/c/abc"))

        func returns(_ string: String) throws -> Bool {
            try ChatGPTAuthSession.isReturnPage(XCTUnwrap(URL(string: string)))
        }
        XCTAssertTrue(try returns("https://chatgpt.com/"))
        XCTAssertTrue(try returns("https://www.chatgpt.com/c/abc"))
        XCTAssertFalse(try returns("https://chatgpt.com/auth/login"))
        XCTAssertFalse(try returns("https://chatgpt.com/signin"))
        XCTAssertFalse(try returns("https://chatgpt.com.evil.example/"))
        XCTAssertFalse(try returns("about:blank"))

        let config = makeAuth(signedIn: false).signInConfig
        XCTAssertEqual(config.title, "Sign in to ChatGPT")
        XCTAssertEqual(config.startURL.absoluteString, "https://chatgpt.com/")
        XCTAssertTrue(config.isAuthHost("auth.openai.com", "/"))
        XCTAssertTrue(config.isReturnPage(try XCTUnwrap(URL(string: "https://chatgpt.com/"))))
    }
}
