import SwiftUI
@testable import TokenMon
import XCTest

/// The Claude panel shows its usage windows, the Claude Code section from
/// this Mac's logs, and the sign-in and sign-out controls for each state.
@MainActor
final class ClaudePanelViewTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var defaults: UserDefaults!
    private let opus = "claude-opus-5-5"
    private let unpriced = "claude-mystery-1"

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "ClaudePanel-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suiteName)
    }

    /// This month: $10 of Opus output and an unpriced model, so the month
    /// spend carries a "+".
    private func codeSummary(now: Date = Date()) -> ClaudeCodeUsageSummary {
        let calendar = ClaudeCodeUsageLedger.dayCalendar
        let today = DayKey.key(for: now, calendar: calendar)
        return ClaudeCodeUsageSummary.make(
            days: [today: [
                opus: ClaudeModelTokens(outputTokens: 500_000, responseCount: 2),
                unpriced: ClaudeModelTokens(inputTokens: 1000, responseCount: 1)
            ]],
            now: now,
            calendar: calendar
        )
    }

    private func response(fiveHour: Double? = 25, weekly: Double? = 60, scope: String? = "Opus", resets: SavedResets? = nil) -> ClaudeUsageResponse {
        ClaudeUsageResponse(
            fiveHour: fiveHour.map { ClaudeUsageWindow(usedPercent: $0, resetsAt: Date().addingTimeInterval(3600)) },
            sevenDay: weekly.map {
                ClaudeUsageWindow(usedPercent: $0, resetsAt: Date().addingTimeInterval(3 * 86400), scopeName: scope)
            },
            savedResets: resets
        )
    }

    private func makeAuth(signedIn: Bool) -> ClaudeAuthSession {
        let auth = ClaudeAuthSession(directory: dir, store: InMemoryCredentialStore(), cookieJars: .isolated())
        if signedIn {
            auth.save(cookieHeader: "sessionKey=live")
        }
        return auth
    }

    private func makePoller(
        _ auth: ClaudeAuthSession,
        code: ClaudeCodeUsageSummary? = nil,
        fetch: @escaping () throws -> ClaudeUsageResponse
    ) -> ClaudeUsagePoller {
        ClaudeUsagePoller(
            settings: AppSettings(defaults: defaults),
            auth: auth,
            hourly: HourlyDeltaActivityStore(
                store: FileBackedStringStore(directory: dir, filenamePrefix: "hourly_"),
                storageKey: "claude"
            ),
            daily: DailyQuotaDeltaStore(
                store: FileBackedStringStore(directory: dir, filenamePrefix: "daily_"),
                storageKey: "claude_daily_usage"
            ),
            fetchUsage: { _ in (try fetch(), Date()) },
            loadCodeUsage: { _ in code.map { ClaudeCodeUsageScan(summary: $0) } }
        )
    }

    private func panel(
        _ poller: ClaudeUsagePoller,
        _ auth: ClaudeAuthSession,
        expanded: Bool = true,
        openSignIn: @escaping () -> Void = {}
    ) -> ClaudePanelView {
        ClaudePanelView(poller: poller, auth: auth, modelsExpanded: .constant(expanded), openSignIn: openSignIn)
    }

    func testSignedOutPanelOffersSignIn() {
        let auth = makeAuth(signedIn: false)
        let poller = makePoller(auth) { self.response() }
        var opened = 0
        let view = panel(poller, auth) { opened += 1 }
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Sign in to claude.ai to load your 5-hour and weekly usage."))
        XCTAssertFalse(tree.shows("Tokens this month"))
        tree.first(ProviderSignInButton.self)?.action()
        XCTAssertEqual(opened, 1)
    }

    func testSignedOutPanelKeepsTheClaudeCodeSection() async {
        let auth = makeAuth(signedIn: false)
        let poller = makePoller(auth, code: codeSummary()) { self.response() }
        await poller.refreshNow()
        XCTAssertNil(poller.snapshot)
        let view = panel(poller, auth)
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Sign in to claude.ai to load your 5-hour and weekly usage."))
        XCTAssertTrue(tree.shows("Tokens this month"))
        XCTAssertTrue(tree.shows("Models This Month"))
    }

    func testUsageWindowsCodeSectionAndSavedResets() async {
        let auth = makeAuth(signedIn: true)
        let resets = SavedResets(count: 2, nextExpiry: Date().addingTimeInterval(5 * 86400))
        let poller = makePoller(auth, code: codeSummary()) { self.response(resets: resets) }
        await poller.refreshNow()
        let view = panel(poller, auth)
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Claude"))
        XCTAssertTrue(tree.shows("Usage Pool Mixed"))
        XCTAssertTrue(tree.shows("5-Hour Window"))
        XCTAssertTrue(tree.shows("Weekly · Opus"))
        XCTAssertEqual(tree.all(SlimUsageTrack.self).map(\.percent), [25, 60])
        XCTAssertTrue(tree.shows("Tokens this month"))
        XCTAssertTrue(tree.shows("Spend this month"))
        XCTAssertTrue(tree.shows("$10.00+"))
        XCTAssertTrue(tree.shows("Resets available"))
        XCTAssertFalse(tree.shows("Sign In Again…"))
        XCTAssertNil(tree.all(StaleDataCaption.self).first)
    }

    func testMissingWindowsShowZeroAndAnAccountWideWeeklyLabel() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { self.response(fiveHour: nil, weekly: 10, scope: nil) }
        await poller.refreshNow()
        let view = panel(poller, auth)
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Weekly"))
        XCTAssertFalse(tree.shows("Weekly ·"))
        XCTAssertEqual(tree.all(SlimUsageTrack.self).map(\.percent), [0, 10])
        XCTAssertNil(tree.all(SlimUsageTrack.self).first?.resetsAt)
        XCTAssertFalse(tree.shows("Resets available"))
        XCTAssertFalse(tree.shows("Tokens this month"))
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
        let view = panel(poller, auth) { opened += 1 }
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertEqual(tree.all(SlimUsageTrack.self).map(\.percent), [25, 60])
        XCTAssertEqual(tree.first(StaleDataCaption.self)?.message, poller.lastError)
        tree.press("Sign In Again…")
        XCTAssertEqual(opened, 1)
    }

    func testSignOutClearsTheSession() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { self.response() }
        await poller.refreshNow()
        ViewTree(panel(poller, auth)).first(ProviderSignOutButton.self)?.action()
        XCTAssertFalse(auth.isSignedIn)
        XCTAssertNil(poller.snapshot)
    }

    func testSignedInWithoutDataShowsStatus() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth, code: codeSummary()) { throw URLError(.notConnectedToInternet) }
        let waiting = panel(poller, auth)
        ViewHarness.render(waiting)
        XCTAssertTrue(ViewTree(waiting).shows("No usage data yet."))
        XCTAssertFalse(ViewTree(waiting).shows("Sign In"))

        await poller.refreshNow()
        let failed = panel(poller, auth)
        ViewHarness.render(failed)
        let tree = ViewTree(failed)
        XCTAssertTrue(tree.shows(poller.lastError ?? "-"))
        XCTAssertTrue(tree.shows("Tokens this month"))
    }

    func testRejectedSessionWithoutDataShowsTheSignedOutPrompt() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth) { throw URLError(.timedOut) }
        await poller.refreshNow()
        auth.markSessionInvalid(reason: "expired")
        XCTAssertTrue(auth.needsSignIn)
        // needsSignIn without data shows the signed-out prompt.
        let view = panel(poller, auth)
        ViewHarness.render(view)
        XCTAssertTrue(ViewTree(view).shows("Sign in to claude.ai"))
    }

    func testCodeSectionStatsAndSpendSuffix() {
        let summary = codeSummary()
        let section = ClaudeCodeUsageSection(usage: summary, modelsExpanded: .constant(true))
        let titles = section.stats.stats.map(\.title)
        XCTAssertEqual(titles, ["Tokens this month", "Spend this month", "Total tokens", "Total spend"])
        XCTAssertEqual(section.stats.stats[1].value, "$10.00+")
        XCTAssertEqual(ClaudeCodeUsageSection.spend(ClaudeUsageTotals(tokens: 1, costUSD: 1234)), "$1,234.00")
        ViewHarness.renderBothAppearances(section)
        XCTAssertTrue(ViewTree(section).shows("Models This Month"))
    }

    func testCodeSectionWithoutModelsHasNoModelsCard() {
        let empty = ClaudeCodeUsageSummary.make(days: [:], now: Date(), calendar: ClaudeCodeUsageLedger.dayCalendar)
        let section = ClaudeCodeUsageSection(usage: empty, modelsExpanded: .constant(true))
        ViewHarness.render(section)
        XCTAssertFalse(ViewTree(section).shows("Models This Month"))
        XCTAssertEqual(section.stats.stats[1].value, "$0.00")
    }

    func testModelsSectionOpensAndClosesTheRows() {
        let summary = codeSummary()
        var expanded = false
        let binding = Binding(get: { expanded }, set: { expanded = $0 })
        let closed = ClaudeModelsSection(usage: summary, isExpanded: binding)
        XCTAssertEqual(closed.header.title, "Models This Month")
        let closedHeight = ViewHarness.render(closed).height
        closed.header.isExpanded.toggle()
        XCTAssertTrue(expanded)
        let open = ClaudeModelsSection(usage: summary, isExpanded: binding)
        XCTAssertGreaterThan(ViewHarness.renderBothAppearances(open).height, closedHeight)
    }

    func testModelRowValueAndVoiceOverLabel() throws {
        let summary = codeSummary()
        let priced = try XCTUnwrap(summary.monthModels.first { $0.modelID == opus })
        let pricedDetail = ClaudeModelHoverDetail(model: priced, summary: summary)
        let row = ClaudeModelRow(model: priced, detail: pricedDetail, isHovered: true)
        ViewHarness.renderBothAppearances(row)
        XCTAssertTrue(ViewTree(row).shows("Opus 5.5"))
        XCTAssertTrue(ViewTree(row).shows("$10.00"))
        XCTAssertTrue(ViewTree(row).shows(pricedDetail.accessibilityLabel))

        let mystery = try XCTUnwrap(summary.monthModels.first { $0.modelID == unpriced })
        let noPrice = ClaudeModelRow(model: mystery, detail: ClaudeModelHoverDetail(model: mystery, summary: summary))
        ViewHarness.render(noPrice)
        XCTAssertTrue(ViewTree(noPrice).shows("No price"))
    }

    func testHoverLabelShowsTheDetailLines() throws {
        let now = Date()
        let calendar = ClaudeCodeUsageLedger.dayCalendar
        let lastMonth = try XCTUnwrap(calendar.date(byAdding: .month, value: -1, to: now))
        let summary = ClaudeCodeUsageSummary.make(
            days: [
                DayKey.key(for: lastMonth, calendar: calendar): [opus: ClaudeModelTokens(outputTokens: 250_000, responseCount: 1)],
                DayKey.key(for: now, calendar: calendar): [opus: ClaudeModelTokens(outputTokens: 500_000, responseCount: 1)]
            ],
            now: now,
            calendar: calendar
        )
        let model = try XCTUnwrap(summary.monthModels.first)
        let detail = ClaudeModelHoverDetail(model: model, summary: summary)
        let withPrevious = try XCTUnwrap(detail.previousLine)
        let label = ClaudeModelHoverLabel(detail: detail)
        let fullHeight = ViewHarness.renderBothAppearances(label).height
        let tree = ViewTree(label)
        XCTAssertTrue(tree.shows(detail.title))
        XCTAssertTrue(tree.shows(withPrevious))
        XCTAssertTrue(tree.shows(detail.changeLine))

        var noHistory = detail
        noHistory.previousLine = nil
        let short = ClaudeModelHoverLabel(detail: noHistory)
        XCTAssertFalse(ViewTree(short).shows(withPrevious))
        XCTAssertLessThan(ViewHarness.render(short).height, fullHeight)
    }

    func testSignInAuthHostsMatchExactlyOrBySubdomainNotSubstring() {
        for host in ["github.com.evil.example", "evilgithub.com", "notclaude.ai.example",
                     "clerk.claude.ai.evil.example", "accounts.google.com.evil.example",
                     "appleid.apple.com.evil.example"] {
            XCTAssertFalse(ClaudeAuthSession.isAuthHost(host: host, path: "/"), host)
        }
        // A login path counts only on claude.ai itself.
        XCTAssertFalse(ClaudeAuthSession.isAuthHost(host: "evil.example", path: "/login"))
        XCTAssertFalse(ClaudeAuthSession.isAuthHost(host: "claude.ai.evil.example", path: "/signin"))
        XCTAssertFalse(ClaudeAuthSession.isAuthHost(host: "notclaude.ai", path: "/login"))

        for host in ["GitHub.com", "accounts.google.com.", "idmsa.apple.com", "foo.clerk.claude.ai", "gist.github.com"] {
            XCTAssertTrue(ClaudeAuthSession.isAuthHost(host: host, path: "/"), host)
        }
        XCTAssertTrue(ClaudeAuthSession.isAuthHost(host: "www.claude.ai", path: "/LOGIN"))
    }

    func testSignInRecognizesAuthHostsAndTheReturnPage() throws {
        XCTAssertTrue(ClaudeAuthSession.isAuthHost(host: "clerk.claude.ai", path: "/"))
        XCTAssertTrue(ClaudeAuthSession.isAuthHost(host: "accounts.google.com", path: "/o/oauth2"))
        XCTAssertTrue(ClaudeAuthSession.isAuthHost(host: "github.com", path: "/login/oauth"))
        XCTAssertTrue(ClaudeAuthSession.isAuthHost(host: "appleid.apple.com", path: "/auth"))
        XCTAssertTrue(ClaudeAuthSession.isAuthHost(host: "claude.ai", path: "/login"))
        XCTAssertTrue(ClaudeAuthSession.isAuthHost(host: "claude.ai", path: "/signin"))
        XCTAssertFalse(ClaudeAuthSession.isAuthHost(host: "claude.ai", path: "/new"))

        XCTAssertTrue(ClaudeAuthSession.isReturnPage(try XCTUnwrap(URL(string: "https://claude.ai/new"))))
        XCTAssertTrue(ClaudeAuthSession.isReturnPage(try XCTUnwrap(URL(string: "https://www.claude.ai/chats"))))
        XCTAssertFalse(ClaudeAuthSession.isReturnPage(try XCTUnwrap(URL(string: "https://claude.ai/login"))))
        XCTAssertFalse(ClaudeAuthSession.isReturnPage(try XCTUnwrap(URL(string: "https://claude.ai/signin"))))
        XCTAssertFalse(ClaudeAuthSession.isReturnPage(try XCTUnwrap(URL(string: "https://claude.ai/auth/callback"))))
        XCTAssertFalse(ClaudeAuthSession.isReturnPage(try XCTUnwrap(URL(string: "https://clerk.claude.ai/v1"))))

        let config = makeAuth(signedIn: false).signInConfig
        XCTAssertEqual(config.title, "Sign in to Claude")
        XCTAssertEqual(config.startURL.absoluteString, "https://claude.ai/new")
        XCTAssertTrue(config.isReturnPage(try XCTUnwrap(URL(string: "https://claude.ai/new"))))
        XCTAssertTrue(config.isAuthHost("clerk.claude.ai", "/"))
    }
}
