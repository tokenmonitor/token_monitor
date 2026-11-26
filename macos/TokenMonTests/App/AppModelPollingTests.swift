@testable import TokenMon
import XCTest

/// AppModel polling wiring under the test host (pollers never start, files and
/// defaults are isolated): menu-open state reaches every poller.
@MainActor
final class AppModelPollingTests: XCTestCase {
    func testMenuOpenStateReachesEveryPoller() {
        let model = AppModel()
        model.setMenuOpen(true)
        XCTAssertTrue(model.providers.all.allSatisfy(\.poller.menuIsOpen))
        model.setMenuOpen(false)
        XCTAssertTrue(model.providers.all.allSatisfy { !$0.poller.menuIsOpen })
    }

    /// Grokbot signs in with Cursor's session, so a Cursor sign-in refreshes
    /// both; every other provider has its own session.
    func testSessionSharingFollowsTheRegistry() {
        let model = AppModel()
        XCTAssertEqual(model.providers.pollers(sharingSessionWith: .cursor).count, 2)
        XCTAssertEqual(model.providers.pollers(sharingSessionWith: .grokbot).count, 2)
        XCTAssertEqual(model.providers.pollers(sharingSessionWith: .claude).count, 1)
    }

    /// Any one stored session (or OpenRouter key) counts as signed in.
    func testAnyStoredSessionCountsAsSignedIn() {
        let signIns: [(AppModel) -> Void] = [
            { $0.grokAuth.save(cookieHeader: "sso=live") },
            { $0.claudeAuth.save(cookieHeader: "sessionKey=live") },
            { $0.chatGPTAuth.save(cookieHeader: "__Secure-next-auth.session-token=live") },
            { $0.cursorAuth.save(cookieHeader: "WorkosCursorSessionToken=live") },
            { $0.openCodeAuth.save(cookieHeader: "auth=live") },
            { $0.openRouterAuth.saveAPIKey("sk-or-v1-live") }
        ]
        for (index, signIn) in signIns.enumerated() {
            let model = AppModel()
            XCTAssertFalse(model.isAnyProviderSignedIn, "\(index)")
            signIn(model)
            XCTAssertTrue(model.isAnyProviderSignedIn, "\(index)")
        }
    }

    /// Turning on a provider's menu bar graph refreshes that provider at
    /// once; signed out, the refresh reports the missing session.
    func testShowingAMenuBarGraphRefreshesThatProvider() async throws {
        let model = AppModel()
        model.settings.menuBarGraphProviders.remove(.grok)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertNil(model.grokPoller.lastError)
        model.settings.menuBarGraphProviders.insert(.grok)
        let deadline = Date().addingTimeInterval(2)
        while model.grokPoller.lastError == nil, Date() < deadline {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(model.grokPoller.lastError, ProviderError.notSignedIn(.grok).localizedDescription)
    }

    /// Grokbot opens Cursor's sign-in window; every other provider its own.
    func testSignInWindowFollowsTheSession() {
        XCTAssertEqual(MonitorProvider.grokbot.signInProvider, .cursor)
        for provider in MonitorProvider.allCases where provider != .grokbot {
            XCTAssertEqual(provider.signInProvider, provider)
        }
    }
}
