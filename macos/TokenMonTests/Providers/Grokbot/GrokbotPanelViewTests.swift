import SwiftUI
@testable import TokenMon
import XCTest

/// The Grokbot panel shows the weekly allowance (or the on-demand note), the
/// usage breakdown, and the Cursor sign-in controls per state.
@MainActor
final class GrokbotPanelViewTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var defaults: UserDefaults!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "GrokbotPanel-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suiteName)
    }

    private func snapshot(allowance: Bool = true) -> GrokbotSnapshot {
        GrokbotSnapshot(
            fetchedAt: Date(),
            usedPercent: 40,
            periodStart: Date().addingTimeInterval(-3 * 86400),
            resetsAt: Date().addingTimeInterval(4 * 86400),
            hasIncludedAllowance: allowance
        )
    }

    private func breakdown(unit: GrokbotUsageBreakdown.Unit = .weeklyLimitPercent) -> GrokbotUsageBreakdown {
        GrokbotUsageBreakdown(
            unit: unit,
            periodStart: Date().addingTimeInterval(-3 * 86400),
            periodEnd: Date().addingTimeInterval(4 * 86400),
            slices: [.init(source: .user, value: 30), .init(source: .automation, value: 10)]
        )
    }

    private func makeAuth(signedIn: Bool) -> CursorAuthSession {
        let auth = CursorAuthSession(directory: dir, store: InMemoryCredentialStore())
        if signedIn {
            auth.save(cookieHeader: "WorkosCursorSessionToken=live")
        }
        return auth
    }

    private func makePoller(
        _ auth: CursorAuthSession,
        breakdown: GrokbotUsageBreakdown? = nil,
        fetch: @escaping () throws -> GrokbotSnapshot
    ) -> GrokbotUsagePoller {
        GrokbotUsagePoller(
            settings: AppSettings(defaults: defaults),
            auth: auth,
            hourly: HourlyDeltaActivityStore(
                store: FileBackedStringStore(directory: dir, filenamePrefix: "hourly_"),
                storageKey: "grokbot"
            ),
            daily: DailyQuotaDeltaStore(
                store: FileBackedStringStore(directory: dir, filenamePrefix: "daily_"),
                storageKey: "grokbot_daily_usage"
            ),
            fetchSnapshot: { _, _ in try fetch() },
            fetchBreakdown: { _ in breakdown }
        )
    }

    func testSignedOutPanelExplainsTheCursorSignIn() {
        let auth = makeAuth(signedIn: false)
        let poller = makePoller(auth) { self.snapshot() }
        var opened = 0
        let view = GrokbotPanelView(poller: poller, auth: auth) { opened += 1 }
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Sign in to Cursor to load your weekly Grokbot allowance. "
                + "Grokbot is billed through Cursor even when a SuperGrok plan pays for it."))
        tree.first(ProviderSignInButton.self)?.action()
        XCTAssertEqual(opened, 1)
    }

    func testAllowanceBreakdownAndBudget() async throws {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth, breakdown: breakdown()) { self.snapshot() }
        await poller.refreshNow()
        let view = GrokbotPanelView(poller: poller, auth: auth) {}
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Grokbot"))
        XCTAssertTrue(tree.shows("40% Used"))
        XCTAssertEqual(tree.all(SlimUsageTrack.self).map(\.percent), [40])
        XCTAssertFalse(tree.shows("no included Bot allowance"))
        let items = try XCTUnwrap(tree.first(UsageBreakdownCard.self)).items
        XCTAssertEqual(items.map(\.label), ["You", "Routines"])
        XCTAssertNotNil(tree.first(NavigableDailyBudgetCard.self))
        // The session is Cursor's, so sign-out confirms for Cursor.
        XCTAssertEqual(tree.first(ProviderSignOutButton.self)?.provider, .cursor)
    }

    func testOnDemandPlanShowsTheNoteWithoutTrackOrBudget() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { self.snapshot(allowance: false) }
        await poller.refreshNow()
        let view = GrokbotPanelView(poller: poller, auth: auth) {}
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("This plan has no included Bot allowance — usage bills on demand."))
        XCTAssertTrue(tree.all(SlimUsageTrack.self).isEmpty)
        XCTAssertTrue(tree.all(NavigableDailyBudgetCard.self).isEmpty)
        XCTAssertTrue(tree.all(UsageBreakdownCard.self).isEmpty)
    }

    func testEndedBreakdownIsHidden() async {
        var ended = breakdown()
        ended.periodEnd = Date().addingTimeInterval(-60)
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth, breakdown: ended) { self.snapshot() }
        await poller.refreshNow()
        XCTAssertNotNil(poller.breakdown)
        XCTAssertTrue(ViewTree(GrokbotPanelView(poller: poller, auth: auth) {}).all(UsageBreakdownCard.self).isEmpty)
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
        let view = GrokbotPanelView(poller: poller, auth: auth) { opened += 1 }
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("40% Used"))
        XCTAssertEqual(tree.first(StaleDataCaption.self)?.message, poller.lastError)
        tree.press("Sign In Again…")
        XCTAssertEqual(opened, 1)
    }

    func testSignOutEndsTheCursorSession() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { self.snapshot() }
        await poller.refreshNow()
        ViewTree(GrokbotPanelView(poller: poller, auth: auth) {}).first(ProviderSignOutButton.self)?.action()
        XCTAssertFalse(auth.isSignedIn)
        XCTAssertNil(poller.snapshot)
    }

    func testSignedInWithoutDataShowsStatus() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { throw URLError(.notConnectedToInternet) }
        let waiting = GrokbotPanelView(poller: poller, auth: auth) {}
        ViewHarness.render(waiting)
        XCTAssertTrue(ViewTree(waiting).shows("No usage data yet."))

        await poller.refreshNow()
        let failed = GrokbotPanelView(poller: poller, auth: auth) {}
        ViewHarness.render(failed)
        XCTAssertTrue(ViewTree(failed).shows(poller.lastError ?? "-"))
    }

    func testBreakdownCardShowsPercentsOrDollars() throws {
        let percent = GrokbotUsageBreakdownCard(breakdown: breakdown())
        ViewHarness.renderBothAppearances(percent)
        let shares = try XCTUnwrap(ViewTree(percent).first(UsageBreakdownCard.self)).items
        XCTAssertEqual(shares.map(\.id), ["user", "automation"])
        XCTAssertEqual(shares.map(\.amount), [30, 10])
        XCTAssertEqual(shares.map(\.valueText), [nil, nil])

        let dollars = GrokbotUsageBreakdownCard(breakdown: breakdown(unit: .usd))
        ViewHarness.render(dollars)
        let spend = try XCTUnwrap(ViewTree(dollars).first(UsageBreakdownCard.self)).items
        XCTAssertEqual(spend.map(\.valueText), ["$30.00", "$10.00"])
    }
}
