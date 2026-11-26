import SwiftUI
@testable import TokenMon
import XCTest

/// The Grok panel shows the right header for each auth state and wires its
/// buttons to sign-in and sign-out.
@MainActor
final class GrokPanelViewTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var defaults: UserDefaults!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "GrokPanel-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suiteName)
    }

    private func makePoller(
        auth: GrokAuthSession,
        fetch: @escaping (String?, String?) async throws -> WeeklyUsageSnapshot
    ) -> GrokUsagePoller {
        GrokUsagePoller(
            auth: auth,
            history: GrokHistoryStore(inMemory: true),
            settings: AppSettings(defaults: defaults),
            grokHourly: HourlyDeltaActivityStore(
                store: FileBackedStringStore(directory: dir, filenamePrefix: "hourly_"),
                storageKey: "grok"
            ),
            fetchUsage: fetch,
            fetchSavedResets: { _ in nil }
        )
    }

    private func panel(_ auth: GrokAuthSession, _ poller: GrokUsagePoller, openSignIn: @escaping () -> Void = {}) -> GrokPanelView {
        GrokPanelView(auth: auth, poller: poller, settings: AppSettings(defaults: defaults), openSignIn: openSignIn)
    }

    func testSignedOutPanelOffersSignIn() {
        let auth = GrokAuthSession(directory: dir)
        let poller = makePoller(auth: auth) { _, _ in WeeklyUsageSnapshot(usedPercent: 0, remainingPercent: 100) }
        var opened = 0
        let view = panel(auth, poller) { opened += 1 }
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("SuperGrok"))
        XCTAssertTrue(tree.shows("Sign in to load your usage."))
        tree.press("Sign In…")
        XCTAssertEqual(opened, 1)
    }

    func testExpiredSessionAsksToSignInAgain() {
        let auth = GrokAuthSession(directory: dir)
        auth.save(cookieHeader: "sso=live")
        auth.markSessionInvalid(reason: "expired")
        let poller = makePoller(auth: auth) { _, _ in WeeklyUsageSnapshot(usedPercent: 0, remainingPercent: 100) }
        var opened = 0
        let view = panel(auth, poller) { opened += 1 }
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Session expired. Sign in again to load usage."))
        tree.press("Sign In Again…")
        XCTAssertEqual(opened, 1)
    }

    func testSignedInWithoutUsageWaitsForData() {
        let auth = GrokAuthSession(directory: dir)
        auth.save(cookieHeader: "sso=live")
        let poller = makePoller(auth: auth) { _, _ in WeeklyUsageSnapshot(usedPercent: 0, remainingPercent: 100) }
        let view = panel(auth, poller)
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Signed in — waiting for usage data."))
        XCTAssertFalse(tree.shows("Sign In"))
    }

    func testSignedInPanelShowsUsageAndSignsOut() async {
        let auth = GrokAuthSession(directory: dir)
        auth.save(cookieHeader: "sso=live")
        let poller = makePoller(auth: auth) { _, _ in
            WeeklyUsageSnapshot(usedPercent: 42, remainingPercent: 58, resetsAt: Date().addingTimeInterval(3600))
        }
        await poller.refreshNow()
        let view = panel(auth, poller)
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Usage Pool Weekly"))
        XCTAssertTrue(tree.shows("42% Used"))
        XCTAssertTrue(auth.isSignedIn)
        tree.first(ProviderSignOutButton.self)?.action()
        XCTAssertFalse(auth.isSignedIn)
    }
}
