import SwiftUI
@testable import TokenMon
import XCTest

/// The Cursor panel shows the plan, its pools, the monthly budget and cost
/// stats per state; its sign-in config finds the dashboard.
@MainActor
final class CursorPanelViewTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var defaults: UserDefaults!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "CursorPanel-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suiteName)
    }

    private func snapshot(pools: [CursorPoolKind] = [.total, .auto, .api], stats: CursorCostStats? = nil) -> CursorSnapshot {
        CursorSnapshot(
            fetchedAt: Date(),
            usedPercent: 42,
            pools: pools.map { CursorPoolUsage(kind: $0, usedPercent: 42, resetsAt: nil) },
            billingCycleStart: Date().addingTimeInterval(-10 * 86400),
            billingCycleEnd: Date().addingTimeInterval(20 * 86400),
            membershipType: "pro",
            planUsedUSD: 8.4,
            planLimitUSD: 20,
            onDemandEnabled: false,
            onDemandUsedUSD: nil,
            onDemandLimitUSD: nil,
            costStats: stats,
            accountEmail: nil
        )
    }

    private func makeAuth(signedIn: Bool) -> CursorAuthSession {
        let auth = CursorAuthSession(directory: dir, store: InMemoryCredentialStore())
        if signedIn {
            auth.save(cookieHeader: "WorkosCursorSessionToken=live")
        }
        return auth
    }

    private func makePoller(_ auth: CursorAuthSession, fetch: @escaping () throws -> CursorSnapshot) -> CursorUsagePoller {
        CursorUsagePoller(
            settings: AppSettings(defaults: defaults),
            auth: auth,
            daily: DailyQuotaDeltaStore(
                store: FileBackedStringStore(directory: dir, filenamePrefix: "daily_"),
                storageKey: "cursor_daily_usage"
            ),
            fetchSnapshot: { _ in
                (try fetch(), CursorDayHourlyUsage(dayStart: Date(), hourWeights: [], quotaHourWeights: []), [:])
            }
        )
    }

    func testSignedOutPanelOffersSignIn() {
        let auth = makeAuth(signedIn: false)
        let poller = makePoller(auth) { self.snapshot() }
        var opened = 0
        let view = CursorPanelView(poller: poller, auth: auth) { opened += 1 }
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Sign in to cursor.com to load Total, Auto, and API usage."))
        tree.press("Sign In to Cursor…")
        XCTAssertEqual(opened, 1)
    }

    func testPlanPoolsBudgetAndStats() async {
        let auth = makeAuth(signedIn: true)
        let stats = CursorCostStats(meteredCycleUSD: 12.5, cycleTokens: 1_500_000, cycleInputTokens: 500_000, cycleOutputTokens: 1_000_000)
        let poller = makePoller(auth) { self.snapshot(stats: stats) }
        await poller.refreshNow()
        let view = CursorPanelView(poller: poller, auth: auth) {}
        let threePools = ViewHarness.renderBothAppearances(view).height
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Cursor Pro"))
        XCTAssertNotNil(tree.first(NavigableDailyBudgetCard.self))
        for text in ["Monthly spend", "$12.50", "Total tokens", "1.5M", "Input tokens", "500K", "Output tokens", "1.0M"] {
            XCTAssertTrue(tree.shows(text), text)
        }
        XCTAssertFalse(tree.shows("Sign In Again…"))

        // Each pool draws its own track (they sit in a ForEach, so compare heights).
        let onePoolPoller = makePoller(auth) { self.snapshot(pools: [.total], stats: stats) }
        await onePoolPoller.refreshNow()
        XCTAssertGreaterThan(threePools, ViewHarness.render(CursorPanelView(poller: onePoolPoller, auth: auth) {}).height)
    }

    func testSnapshotWithoutCostStatsHasNoStatsGrid() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { self.snapshot() }
        await poller.refreshNow()
        let tree = ViewTree(CursorPanelView(poller: poller, auth: auth) {})
        XCTAssertTrue(tree.all(CursorStatsGrid.self).isEmpty)
        XCTAssertFalse(tree.shows("Monthly spend"))
    }

    func testExpiredSessionKeepsDataAndAsksToSignInAgain() async {
        let auth = makeAuth(signedIn: true)
        var fail = false
        let poller = makePoller(auth) {
            if fail { throw URLError(.timedOut) }
            return self.snapshot()
        }
        await poller.refreshNow()
        fail = true
        await poller.refreshNow()
        auth.markSessionInvalid(reason: "expired")
        var opened = 0
        let view = CursorPanelView(poller: poller, auth: auth) { opened += 1 }
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Cursor Pro"))
        XCTAssertEqual(tree.first(StaleDataCaption.self)?.message, poller.lastError)
        tree.press("Sign In Again…")
        XCTAssertEqual(opened, 1)
    }

    func testSignOutClearsTheSession() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { self.snapshot() }
        await poller.refreshNow()
        ViewTree(CursorPanelView(poller: poller, auth: auth) {}).first(ProviderSignOutButton.self)?.action()
        XCTAssertFalse(auth.isSignedIn)
        XCTAssertNil(poller.snapshot)
    }

    func testSignedInWithoutDataShowsStatus() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { throw URLError(.notConnectedToInternet) }
        let waiting = CursorPanelView(poller: poller, auth: auth) {}
        ViewHarness.render(waiting)
        XCTAssertTrue(ViewTree(waiting).shows("No usage data yet."))

        await poller.refreshNow()
        let failed = CursorPanelView(poller: poller, auth: auth) {}
        ViewHarness.render(failed)
        XCTAssertTrue(ViewTree(failed).shows(poller.lastError ?? "-"))
    }

    func testSignInFindsIdentityHostsAndTheDashboard() throws {
        let config = makeAuth(signedIn: false).signInConfig
        XCTAssertEqual(config.title, "Sign in to Cursor")
        XCTAssertEqual(config.startURL.absoluteString, "https://cursor.com/dashboard/usage")
        XCTAssertTrue(config.isAuthHost("authenticator.cursor.sh", "/"))
        XCTAssertTrue(config.isAuthHost("cursor.com", "/login"))
        XCTAssertFalse(config.isAuthHost("github.com.evil.example", "/"))

        func returns(_ string: String) throws -> Bool {
            try config.isReturnPage(XCTUnwrap(URL(string: string)))
        }
        XCTAssertTrue(try returns("https://cursor.com/dashboard/usage"))
        XCTAssertTrue(try returns("https://www.cursor.com/settings"))
        XCTAssertFalse(try returns("https://cursor.com/pricing"))
        XCTAssertFalse(try returns("https://cursor.com.evil.example/dashboard"))
        XCTAssertFalse(try returns("about:blank"))
    }
}
