import SwiftUI
@testable import TokenMon
import XCTest

/// The Overview panel shows one card per enabled provider with its headline
/// usage, a waiting caption before data arrives, and sign-in / sign-out
/// controls wired to the right session.
@MainActor
final class OverviewPanelViewTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var defaults: UserDefaults!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "OverviewPanel-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suiteName)
    }

    // MARK: - Signed out

    func testSignedOutPanelWaitsForDataAndOffersEverySignIn() {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        var opened: [MonitorProvider] = []
        var openRouterSelected = 0
        var preferencesOpened = 0
        let view = fixture.panel(
            openSignIn: { opened.append($0) },
            selectOpenRouter: { openRouterSelected += 1 },
            openPreferences: { preferencesOpened += 1 }
        )
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)

        XCTAssertTrue(tree.shows("TokenMon"))
        for name in ["Grok", "OpenCode", "Cursor", "Claude", "ChatGPT", "OpenRouter", "Grokbot"] {
            XCTAssertTrue(tree.shows("No \(name) data yet"), name)
        }
        XCTAssertTrue(tree.shows("0% Used"))
        XCTAssertTrue(tree.all(ProviderSignOutButton.self).isEmpty)

        let signIns = tree.all(ProviderSignInButton.self)
        XCTAssertEqual(
            Set(signIns.map(\.provider)),
            [.grok, .opencode, .cursor, .claude, .chatgpt, .openrouter, .grokbot]
        )
        for button in signIns {
            button.action()
        }
        // Grokbot signs in through Cursor; OpenRouter switches to its tab for the key.
        XCTAssertEqual(opened.sorted(by: { $0.rawValue < $1.rawValue }), [
            .chatgpt, .claude, .cursor, .cursor, .grok, .opencode
        ].sorted(by: { $0.rawValue < $1.rawValue }))
        XCTAssertEqual(openRouterSelected, 1)
        XCTAssertEqual(signIns.first { $0.provider == .grokbot }?.title, "Sign In to Cursor…")

        tree.first(SettingsCogButton.self)?.action()
        XCTAssertEqual(preferencesOpened, 1)
    }

    /// A Grok session that the server rejected still shows the sign-in button
    /// and no sign-out, even though cookies remain on disk.
    func testExpiredGrokSessionAsksToSignInAgain() {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        fixture.grokAuth.save(cookieHeader: "sso=live")
        fixture.grokAuth.markSessionInvalid(reason: "expired")
        let tree = ViewTree(fixture.panel())
        XCTAssertTrue(tree.all(ProviderSignInButton.self).contains { $0.provider == .grok })
        XCTAssertFalse(tree.all(ProviderSignOutButton.self).contains { $0.provider == .grok })
    }

    // MARK: - Signed in

    func testSignedInPanelShowsEachProvidersUsage() async {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        fixture.signInAll()
        await fixture.refreshAll()
        let view = fixture.panel()
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)

        for used in [42, 50, 61, 73, 27, 25, 38] {
            XCTAssertTrue(tree.shows("\(used)% Used"), "\(used)% Used")
        }
        XCTAssertFalse(tree.shows("data yet"))
        XCTAssertTrue(tree.all(ProviderSignInButton.self).isEmpty)
        XCTAssertEqual(
            Set(tree.all(UsageLimitCaption.self).map(\.percent)),
            [42, 50, 61, 73, 27, 25, 38]
        )
        let openRouterCaption = tree.all(UsageLimitCaption.self).first { $0.percent == 25 }
        XCTAssertEqual(openRouterCaption?.normalCaption, "\(Format.usd(75)) of credits left")
    }

    /// Each sign-out button signs out its own provider's session. Grokbot has
    /// none: it shares Cursor's session, which the Cursor card signs out.
    func testSignOutButtonsSignOutTheirOwnSession() async {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        fixture.signInAll()
        await fixture.refreshAll()
        let signOuts = ViewTree(fixture.panel()).all(ProviderSignOutButton.self)
        XCTAssertEqual(Set(signOuts.map(\.provider)), [.grok, .opencode, .cursor, .claude, .chatgpt, .openrouter])

        let sessions: [MonitorProvider: () -> Bool] = [
            .grok: { fixture.grokAuth.isSignedIn },
            .opencode: { fixture.openCodeAuth.isSignedIn },
            .cursor: { fixture.cursorAuth.isSignedIn },
            .claude: { fixture.claudeAuth.isSignedIn },
            .chatgpt: { fixture.chatGPTAuth.isSignedIn },
            .openrouter: { fixture.openRouterAuth.isSignedIn }
        ]
        for button in signOuts {
            XCTAssertEqual(sessions[button.provider]?(), true, "\(button.provider)")
            button.action()
            XCTAssertEqual(sessions[button.provider]?(), false, "\(button.provider)")
        }
    }

    /// A provider whose data is already loaded hides its sign-in button while
    /// signed out (OpenCode's local estimate needs no console session).
    func testLoadedOpenCodeEstimateHidesSignInWhileSignedOut() async {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        await fixture.openCodePoller.refreshNow()
        XCTAssertNotNil(fixture.openCodePoller.snapshot)
        let tree = ViewTree(fixture.panel())
        XCTAssertTrue(tree.shows("50% Used"))
        XCTAssertFalse(tree.all(ProviderSignInButton.self).contains { $0.provider == .opencode })
        XCTAssertFalse(tree.all(ProviderSignOutButton.self).contains { $0.provider == .opencode })
    }

    // MARK: - OpenRouter caption

    /// A key without a credit limit shows its spend in place of a percent.
    func testUnlimitedOpenRouterKeyShowsSpend() async {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        fixture.openRouterSnapshot = OverviewPanelFixture.openRouter(budgetUSD: nil, usedUSD: 12.5, remainingUSD: nil)
        fixture.openRouterAuth.saveAPIKey("sk-or-v1-live")
        await fixture.openRouterPoller.refreshNow()
        let view = fixture.panel()
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("No credit limit — \(Format.usd(12.5)) spent on this key"))
        XCTAssertTrue(tree.shows(Format.usd(12.5)))
        XCTAssertFalse(tree.shows("No OpenRouter data yet"))
    }

    /// A budget without a remaining figure shows neither caption.
    func testOpenRouterBudgetWithoutRemainingShowsNoCaption() async {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        fixture.openRouterSnapshot = OverviewPanelFixture.openRouter(budgetUSD: 100, usedUSD: 10, remainingUSD: nil)
        fixture.openRouterAuth.saveAPIKey("sk-or-v1-live")
        await fixture.openRouterPoller.refreshNow()
        let tree = ViewTree(fixture.panel())
        XCTAssertTrue(tree.shows("10% Used"))
        XCTAssertFalse(tree.shows("No credit limit"))
        XCTAssertFalse(tree.shows("No OpenRouter data yet"))
        XCTAssertFalse(tree.all(UsageLimitCaption.self).contains { $0.percent == 10 })
    }

    // MARK: - Visibility

    /// Only enabled providers get a card, in the saved order.
    func testDisabledProvidersHaveNoCard() {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        fixture.settings.enabledProviderIDs = [.claude, .cursor]
        let view = fixture.panel()
        let narrow = ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("No Claude data yet"))
        XCTAssertTrue(tree.shows("No Cursor data yet"))
        XCTAssertFalse(tree.shows("No Grok data yet"))
        XCTAssertFalse(tree.shows("No OpenRouter data yet"))

        fixture.settings.enabledProviderIDs = Set(MonitorProvider.usageProviders)
        XCTAssertGreaterThan(ViewHarness.render(fixture.panel()).height, narrow.height)
    }

    func testHeadlineLabels() {
        XCTAssertEqual(OverviewPanelView.usedLabel(41.6), "42% Used")
        XCTAssertEqual(OverviewPanelView.openRouterHeadline(nil), "0% Used")
        let unlimited = OverviewPanelFixture.openRouter(budgetUSD: nil, usedUSD: 3, remainingUSD: nil)
        XCTAssertEqual(OverviewPanelView.openRouterHeadline(unlimited), Format.usd(3))
        let capped = OverviewPanelFixture.openRouter(budgetUSD: 20, usedUSD: 5, remainingUSD: 15)
        XCTAssertEqual(OverviewPanelView.openRouterHeadline(capped), "25% Used")
    }

    // MARK: - Daily usage card

    private func hours(_ values: [Int: Double]) -> [Double] {
        (0..<24).map { values[$0] ?? 0 }
    }

    private func dailyCard(
        _ fixture: OverviewPanelFixture,
        today: OverviewDaySeries = OverviewDaySeries(),
        dayOffset: Int = 0
    ) -> OverviewDailyUsageCard {
        OverviewDailyUsageCard(now: Date(), history: fixture.history, todaySeries: { _ in today }, dayOffset: dayOffset)
    }

    /// The panel ends with today's chart; with nothing recorded it says so and
    /// offers no day arrows.
    func testTodayWithoutActivityShowsTheEmptyMessage() {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        let card = dailyCard(fixture)
        ViewHarness.render(card)
        let tree = ViewTree(card)
        XCTAssertTrue(tree.shows("Usage Today"))
        XCTAssertTrue(tree.shows("No Grok, Grokbot, OpenCode, Cursor, or Claude activity yet today."))
        XCTAssertTrue(tree.all(WeekChartNavigation.self).isEmpty)
    }

    /// Today's activity is charted, and a saved earlier day enables the back
    /// arrow but not the forward one.
    func testTodayWithHistoryChartsActivityAndCanGoBack() {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        var yesterday = OverviewDaySeries()
        yesterday.weights[.claude] = hours([10: 2])
        fixture.history.record(yesterday, at: Calendar.current.date(byAdding: .day, value: -1, to: Date()) ?? Date())
        var today = OverviewDaySeries()
        today.weights[.grok] = hours([9: 1.5])
        let card = dailyCard(fixture, today: today)
        let charted = ViewHarness.render(card)
        let tree = ViewTree(card)
        XCTAssertTrue(tree.shows("Usage Today"))
        XCTAssertTrue(tree.shows("Grok 1.50% of weekly"))
        XCTAssertFalse(tree.shows("activity yet today"))
        let navigation = tree.first(WeekChartNavigation.self)
        XCTAssertNotNil(navigation?.onPreviousWeek)
        XCTAssertEqual(navigation?.canGoNext, false)
        // The arrows step the card's own day; outside a render they change nothing.
        navigation?.onPreviousWeek?()
        navigation?.onNextWeek?()

        let empty = ViewHarness.render(dailyCard(fixture))
        XCTAssertGreaterThan(charted.height, empty.height)
    }

    /// Yesterday reads the saved history, can step forward, and cannot step
    /// back past the earliest saved day.
    func testYesterdayShowsSavedHistory() {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        var yesterday = OverviewDaySeries()
        yesterday.weights[.claude] = hours([10: 2])
        fixture.history.record(yesterday, at: Calendar.current.date(byAdding: .day, value: -1, to: Date()) ?? Date())
        let card = dailyCard(fixture, dayOffset: -1)
        ViewHarness.render(card)
        let tree = ViewTree(card)
        XCTAssertTrue(tree.shows("Usage Yesterday"))
        XCTAssertTrue(tree.shows("Claude 2.00% of 5-hour window"))
        let navigation = tree.first(WeekChartNavigation.self)
        XCTAssertNil(navigation?.onPreviousWeek)
        XCTAssertEqual(navigation?.canGoNext, true)
    }

    /// An earlier day with nothing saved shows its date and the past-day message.
    func testEarlierDayWithoutDataShowsThePastDayMessage() {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        let card = dailyCard(fixture, dayOffset: -3)
        let tree = ViewTree(card)
        XCTAssertTrue(tree.shows("No activity recorded on this day."))
        XCTAssertFalse(tree.shows("Usage Today"))
        XCTAssertFalse(tree.shows("Usage Yesterday"))
        XCTAssertNotNil(tree.first(WeekChartNavigation.self))
    }
}
