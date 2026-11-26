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
        let auth = GrokAuthSession(directory: dir, cookieJars: .isolated())
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
        let auth = GrokAuthSession(directory: dir, cookieJars: .isolated())
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
        let auth = GrokAuthSession(directory: dir, cookieJars: .isolated())
        auth.save(cookieHeader: "sso=live")
        let poller = makePoller(auth: auth) { _, _ in WeeklyUsageSnapshot(usedPercent: 0, remainingPercent: 100) }
        let view = panel(auth, poller)
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Signed in — waiting for usage data."))
        XCTAssertFalse(tree.shows("Sign In"))
    }

    func testSignedInPanelShowsUsageAndSignsOut() async {
        let auth = GrokAuthSession(directory: dir, cookieJars: .isolated())
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

    private func loadedPoller(
        _ auth: GrokAuthSession,
        products: [ProductUsage],
        resets: SavedResets? = nil
    ) async -> (GrokUsagePoller, () -> Void) {
        var fail = false
        let poller = GrokUsagePoller(
            auth: auth,
            history: GrokHistoryStore(inMemory: true),
            settings: AppSettings(defaults: defaults),
            grokHourly: HourlyDeltaActivityStore(
                store: FileBackedStringStore(directory: dir, filenamePrefix: "hourly_"),
                storageKey: "grok"
            ),
            fetchUsage: { _, _ in
                if fail { throw URLError(.timedOut) }
                return WeeklyUsageSnapshot(usedPercent: 30, resetsAt: Date().addingTimeInterval(3 * 86400), products: products)
            },
            fetchSavedResets: { _ in resets }
        )
        await poller.refreshNow()
        return (poller, { fail = true })
    }

    func testUsageShowsBreakdownChartAndSavedResets() async throws {
        let auth = GrokAuthSession(directory: dir, cookieJars: .isolated())
        auth.save(cookieHeader: "sso=live")
        let products = [
            ProductUsage(id: "chat", displayName: "Chat", percentOfPool: 10),
            ProductUsage(id: "build", displayName: "Grok Build", percentOfPool: 20)
        ]
        let resets = SavedResets(count: 1, nextExpiry: nil)
        let (poller, _) = await loadedPoller(auth, products: products, resets: resets)
        let view = panel(auth, poller)
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("30% Used"))
        XCTAssertTrue(tree.shows("Resets available"))
        let items = try XCTUnwrap(tree.first(UsageBreakdownCard.self)).items
        XCTAssertEqual(items.map(\.label), ["Build", "Chat"])
        XCTAssertEqual(items.map(\.amount), [20, 10])

        let chart = try XCTUnwrap(tree.first(DailyUsageChartView.self))
        XCTAssertEqual(chart.periodUsedPercent, 30)
        XCTAssertNotNil(chart.resetsAt)
        XCTAssertFalse(chart.canGoNext)
        chart.onPreviousWeek?()
        chart.onNextWeek?()
        XCTAssertTrue(tree.all(StaleDataCaption.self).isEmpty)
    }

    func testBreakdownCardIsHiddenWithoutProductSplit() async {
        let auth = GrokAuthSession(directory: dir, cookieJars: .isolated())
        auth.save(cookieHeader: "sso=live")
        let (poller, _) = await loadedPoller(auth, products: [])
        let tree = ViewTree(panel(auth, poller))
        XCTAssertTrue(tree.all(UsageBreakdownCard.self).isEmpty)
        XCTAssertFalse(tree.shows("Resets available"))
    }

    func testPastWeekChartHasNoLivePaceAndCanGoForward() async throws {
        let auth = GrokAuthSession(directory: dir, cookieJars: .isolated())
        auth.save(cookieHeader: "sso=live")
        let (poller, _) = await loadedPoller(auth, products: [])
        let view = GrokPanelView(auth: auth, poller: poller, settings: AppSettings(defaults: defaults), openSignIn: {}, weekOffset: -1)
        ViewHarness.render(view)
        let chart = try XCTUnwrap(ViewTree(view).first(DailyUsageChartView.self))
        XCTAssertNil(chart.periodUsedPercent)
        XCTAssertNil(chart.resetsAt)
        XCTAssertTrue(chart.canGoNext)
    }

    func testFailedRefreshKeepsUsageWithAStaleCaption() async {
        let auth = GrokAuthSession(directory: dir, cookieJars: .isolated())
        auth.save(cookieHeader: "sso=live")
        let (poller, failNext) = await loadedPoller(auth, products: [])
        failNext()
        await poller.refreshNow()
        let view = panel(auth, poller)
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("30% Used"))
        XCTAssertEqual(tree.first(StaleDataCaption.self)?.message, poller.lastError)
    }

    func testExpiredSessionShowsThePollerError() async {
        let auth = GrokAuthSession(directory: dir, cookieJars: .isolated())
        auth.save(cookieHeader: "sso=live")
        let (poller, failNext) = await loadedPoller(auth, products: [])
        failNext()
        await poller.refreshNow()
        auth.markSessionInvalid(reason: "expired")
        let tree = ViewTree(panel(auth, poller))
        XCTAssertTrue(tree.shows(poller.lastError ?? "-"))
        XCTAssertFalse(tree.shows("Session expired. Sign in again to load usage."))
    }

    func testSignedInSessionNeedingSignInWaitsWithASignInButton() {
        let auth = GrokAuthSession(directory: dir, cookieJars: .isolated())
        auth.save(cookieHeader: "sso=live")
        auth.needsSignIn = true
        XCTAssertNil(auth.lastAuthError)
        let poller = makePoller(auth: auth) { _, _ in WeeklyUsageSnapshot(usedPercent: 0, remainingPercent: 100) }
        var opened = 0
        let view = panel(auth, poller) { opened += 1 }
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Signed in — waiting for usage data."))
        tree.press("Sign In Again…")
        XCTAssertEqual(opened, 1)
    }

    func testBreakdownCardMapsProductsToShortNames() throws {
        let card = GrokUsageBreakdownCard(products: [
            ProductUsage(id: "imagine", displayName: "Imagine", percentOfPool: 12.5),
            ProductUsage(id: "api", displayName: "API", percentOfPool: 3)
        ])
        ViewHarness.renderBothAppearances(card)
        let items = try XCTUnwrap(ViewTree(card).first(UsageBreakdownCard.self)).items
        XCTAssertEqual(items.map(\.id), ["imagine", "api"])
        XCTAssertEqual(items.map(\.label), ["Imagine", "API"])
        XCTAssertEqual(items.map(\.amount), [12.5, 3])
        XCTAssertNil(items[0].valueText)
    }

    func testSignInMatchesXAIHostsAndTheGrokReturnPage() throws {
        XCTAssertTrue(GrokAuthSession.isAuthHost("accounts.x.ai"))
        XCTAssertTrue(GrokAuthSession.isAuthHost("x.com"))
        XCTAssertTrue(GrokAuthSession.isAuthHost("mobile.twitter.com"))
        XCTAssertFalse(GrokAuthSession.isAuthHost("netflix.com"))
        XCTAssertFalse(GrokAuthSession.isAuthHost("grok.com"))
        XCTAssertTrue(GrokAuthSession.isReturnPage(try XCTUnwrap(URL(string: "https://grok.com/?_s=usage"))))
        XCTAssertFalse(GrokAuthSession.isReturnPage(try XCTUnwrap(URL(string: "https://grok.com.evil.example/"))))
        XCTAssertFalse(GrokAuthSession.isReturnPage(try XCTUnwrap(URL(string: "about:blank"))))

        let config = GrokAuthSession(directory: dir, cookieJars: .isolated()).signInConfig
        XCTAssertEqual(config.title, "Sign in to Grok")
        XCTAssertEqual(config.startURL.host, "accounts.x.ai")
        XCTAssertEqual(config.returnDelayNanoseconds, 1_500_000_000)
        XCTAssertTrue(config.isAuthHost("auth.x.ai", "/"))
        XCTAssertFalse(config.isAuthHost("grok.com", "/"))
        XCTAssertTrue(config.isReturnPage(try XCTUnwrap(URL(string: "https://grok.com/"))))
    }

    func testDailyUsagePreviewIsAFullThursdayWeek() {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC") ?? .current
        let week = GrokDailyUsageBuilder.preview(calendar: calendar)
        XCTAssertEqual(week.days.count, 7)
        XCTAssertFalse(week.isEstimated)
        XCTAssertEqual(week.days.first.map { calendar.component(.weekday, from: $0.dayStart) }, 5)
        XCTAssertEqual(week.days.map(\.segments.count), [2, 3, 2, 0, 3, 2, 1])
        XCTAssertEqual(week.days[4].segments.map(\.productID), ["build", "api", "chat"])
        XCTAssertEqual(week.days[4].segments.map(\.percentOfWeekly), [18, 5, 1])
        XCTAssertTrue(week.hasDailyData)
    }
}
