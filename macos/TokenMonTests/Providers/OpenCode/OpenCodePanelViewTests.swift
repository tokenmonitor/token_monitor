import SwiftUI
@testable import TokenMon
import XCTest

/// The OpenCode Go panel shows its limit bars, local estimate note, models
/// and stats per state; its sign-in config captures the console session.
@MainActor
final class OpenCodePanelViewTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var defaults: UserDefaults!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "OpenCodePanel-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suiteName)
    }

    private func model(_ id: String, cost: Double, estimated: Bool = false, unpriced: Bool = false) -> OpenCodeModelUsage {
        OpenCodeModelUsage(
            providerID: "opencode-go",
            modelID: id,
            sessionCount: 1,
            inputTokens: 1000,
            outputTokens: 2000,
            cacheReadTokens: 0,
            cacheWriteTokens: 0,
            costUSD: cost,
            percentOfWindow: 40,
            isCostEstimated: estimated,
            isCostUnpriced: unpriced
        )
    }

    /// Local estimate: idle 5-hour window, weekly and monthly with resets,
    /// three models and the month's stats.
    private func localSnapshot(models: [OpenCodeModelUsage]? = nil) -> OpenCodeSnapshot {
        OpenCodeSnapshot(
            windows: [
                OpenCodeWindowUsage(kind: .rolling5h, usedUSD: 0, limitUSD: 12, resetsAt: nil, sessionCount: 0),
                OpenCodeWindowUsage(kind: .weekly, usedUSD: 6, limitUSD: 30, resetsAt: Date().addingTimeInterval(2 * 86400), sessionCount: 2),
                OpenCodeWindowUsage(kind: .monthly, usedUSD: 3, limitUSD: 60, resetsAt: Date().addingTimeInterval(10 * 86400), sessionCount: 2)
            ],
            models: models ?? [model("glm-5.3", cost: 1.5), model("kimi-k3", cost: 0, unpriced: true), model("qwen-4", cost: 2, estimated: true)],
            isEstimated: true,
            monthlyTokens: 3000,
            monthlyEstimatedUSD: 3,
            monthlyInputTokens: 1000,
            monthlyOutputTokens: 2000
        )
    }

    private func makeAuth(signedIn: Bool) -> OpenCodeAuthSession {
        let auth = OpenCodeAuthSession(directory: dir, store: InMemoryCredentialStore())
        if signedIn {
            auth.save(cookieHeader: "auth=live")
        }
        return auth
    }

    private func makePoller(
        _ auth: OpenCodeAuthSession,
        console: @escaping () throws -> OpenCodeSnapshot = { throw URLError(.timedOut) },
        local: OpenCodeSnapshot?
    ) -> OpenCodeUsagePoller {
        OpenCodeUsagePoller(
            settings: AppSettings(defaults: defaults),
            auth: auth,
            fetchConsole: { _, _ in (try console(), "wrk_test") },
            fetchLocal: { (local, nil) }
        )
    }

    private func panel(_ poller: OpenCodeUsagePoller, _ auth: OpenCodeAuthSession, openSignIn: @escaping () -> Void = {}) -> OpenCodePanelView {
        OpenCodePanelView(poller: poller, auth: auth, modelsExpanded: .constant(true), openSignIn: openSignIn)
    }

    func testSignedOutPanelOffersConsoleSignIn() {
        let auth = makeAuth(signedIn: false)
        let poller = makePoller(auth, local: nil)
        var opened = 0
        let view = panel(poller, auth) { opened += 1 }
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("OpenCode Go"))
        XCTAssertTrue(tree.shows("Sign in to the OpenCode console to load official rolling, weekly, and monthly usage."))
        tree.press("Sign In to OpenCode")
        XCTAssertEqual(opened, 1)
    }

    func testLocalEstimateShowsBarsNoteModelsAndStats() async {
        let auth = makeAuth(signedIn: false)
        let poller = makePoller(auth, local: localSnapshot())
        await poller.refreshNow()
        XCTAssertNotNil(poller.snapshot)
        // A signed-out panel with a local estimate shows the data, not the prompt.
        auth.needsSignIn = false
        let view = panel(poller, auth)
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Estimated from local sessions — sign in for console accuracy."))
        XCTAssertTrue(tree.shows("Top Models by Spend"))
        for text in ["Monthly spend", "$3.00", "Total tokens", "3K", "Input tokens", "1K", "Output tokens", "2K"] {
            XCTAssertTrue(tree.shows(text), text)
        }
        XCTAssertFalse(tree.shows("Sign In to OpenCode…"))
    }

    func testConsoleSnapshotWithoutLocalStatsHidesStatsAndModels() async {
        let auth = makeAuth(signedIn: true)
        var console = localSnapshot(models: [])
        console.isEstimated = false
        console.monthlyTokens = 0
        console.monthlyEstimatedUSD = 0
        let poller = makePoller(auth, console: { console }, local: nil)
        await poller.refreshNow()
        XCTAssertEqual(poller.snapshot?.isEstimated, false)
        let view = panel(poller, auth)
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertFalse(tree.shows("Estimated from local sessions"))
        XCTAssertFalse(tree.shows("Monthly spend"))
        XCTAssertFalse(tree.shows("Top Models by Spend"))
        XCTAssertNotNil(tree.first(ProviderSignOutButton.self))
    }

    func testExpiredSessionShowsTheErrorAndSignIn() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth, console: { throw ProviderError.unauthorized(.openCode) }, local: localSnapshot())
        for _ in 1 ... ProviderAuthSession.authFailureThreshold {
            await poller.refreshNow()
        }
        XCTAssertTrue(auth.needsSignIn)
        var opened = 0
        let view = panel(poller, auth) { opened += 1 }
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Console session expired — showing local estimate. Sign in again for official numbers."))
        tree.press("Sign In to OpenCode…")
        XCTAssertEqual(opened, 1)
    }

    func testSignOutClearsTheSnapshot() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth, console: { self.localSnapshot() }, local: nil)
        await poller.refreshNow()
        ViewTree(panel(poller, auth)).first(ProviderSignOutButton.self)?.action()
        XCTAssertFalse(auth.isSignedIn)
        XCTAssertNil(poller.snapshot)
    }

    func testSignedInWithoutDataShowsStatus() async {
        let auth = makeAuth(signedIn: true)
        let poller = makePoller(auth, local: nil)
        let waiting = panel(poller, auth)
        ViewHarness.render(waiting)
        XCTAssertTrue(ViewTree(waiting).shows("No usage data yet."))

        await poller.refreshNow()
        let error = try? XCTUnwrap(poller.lastError)
        let failed = panel(poller, auth)
        ViewHarness.render(failed)
        XCTAssertTrue(ViewTree(failed).shows(error ?? "-"))
        XCTAssertFalse(ViewTree(failed).shows("Sign In"))
    }

    func testStatsRowGrid() {
        let row = OpenCodeStatsRow(monthlySpendUSD: 12.5, totalTokens: 1_500_000, inputTokens: 500_000, outputTokens: 1_000_000)
        XCTAssertEqual(row.grid.stats.map(\.title), ["Monthly spend", "Total tokens", "Input tokens", "Output tokens"])
        XCTAssertEqual(row.grid.stats.map(\.value), ["$12.50", "1.5M", "500K", "1.0M"])
        XCTAssertEqual(row.grid.source, .thisComputer)
        ViewHarness.render(row)
    }

    func testModelsSectionPreviewsThreeAndToggles() {
        let models = (1 ... 4).map { model("model-\($0)", cost: Double(5 - $0)) }
        var expanded = true
        let binding = Binding(get: { expanded }, set: { expanded = $0 })
        let section = OpenCodeModelsSection(models: models, isExpanded: binding)
        XCTAssertEqual(section.header.title, "Top Models by Spend")
        let openHeight = ViewHarness.renderBothAppearances(section).height
        let tree = ViewTree(section)
        XCTAssertTrue(tree.shows("View all"))
        tree.press("View all")

        section.header.isExpanded.toggle()
        XCTAssertFalse(expanded)
        let closed = OpenCodeModelsSection(models: models, isExpanded: binding)
        XCTAssertFalse(ViewTree(closed).shows("View all"))
        XCTAssertLessThan(ViewHarness.render(closed).height, openHeight)

        let all = OpenCodeModelsSection(models: models, isExpanded: .constant(true), showAll: true)
        XCTAssertTrue(ViewTree(all).shows("Show less"))
        XCTAssertGreaterThan(ViewHarness.render(all).height, openHeight)

        let few = OpenCodeModelsSection(models: Array(models.prefix(2)), isExpanded: .constant(true))
        XCTAssertFalse(ViewTree(few).shows("View all"))
    }

    func testModelRowCostLabels() {
        func label(_ usage: OpenCodeModelUsage) -> ViewTree {
            let row = OpenCodeModelWeekRow(model: usage)
            ViewHarness.render(row)
            return ViewTree(row)
        }
        XCTAssertTrue(label(model("glm-5.3", cost: 1.5)).shows("$1.50"))
        XCTAssertTrue(label(model("kimi-k3", cost: 0, unpriced: true)).shows("No price"))
        XCTAssertTrue(label(model("qwen-4", cost: 2, estimated: true)).shows("~$2.00"))
        XCTAssertTrue(label(model("qwen-4", cost: 0.75, unpriced: true)).shows("~$0.75"))
    }

    func testLimitBarCaptions() throws {
        func track(_ kind: OpenCodeWindowKind, used: Double, resetsAt: Date?) throws -> SlimUsageTrack {
            let bar = OpenCodeLimitBar(window: OpenCodeWindowUsage(kind: kind, usedUSD: used, limitUSD: 10, resetsAt: resetsAt, sessionCount: 1))
            ViewHarness.render(bar)
            return try XCTUnwrap(ViewTree(bar).first(SlimUsageTrack.self))
        }
        let idle = try track(.rolling5h, used: 0, resetsAt: nil)
        XCTAssertEqual(idle.label, "5-Hour")
        XCTAssertEqual(idle.caption, "No usage in last 5h")
        XCTAssertNil(try track(.rolling5h, used: 5, resetsAt: nil).caption)
        XCTAssertNil(try track(.weekly, used: 0, resetsAt: nil).caption)

        let soon = Date().addingTimeInterval(2 * 3600 + 60)
        let rolling = try track(.rolling5h, used: 1, resetsAt: soon)
        XCTAssertEqual(rolling.caption, "Resets \(soon.formatted(.relative(presentation: .named)))")
        XCTAssertEqual(rolling.percent, 10)

        let weekly = try track(.weekly, used: 5, resetsAt: soon)
        XCTAssertEqual(weekly.label, "Weekly")
        XCTAssertEqual(weekly.caption, Format.resetCaption(soon, dateFormat: "EEE h:mma"))
        let monthly = try track(.monthly, used: 5, resetsAt: soon)
        XCTAssertEqual(monthly.label, "Monthly")
        XCTAssertEqual(monthly.caption, Format.resetCaption(soon))
    }

    func testSignInRecognizesConsolePages() throws {
        let config = makeAuth(signedIn: false).signInConfig
        XCTAssertEqual(config.title, "Sign in to OpenCode")
        XCTAssertEqual(config.startURL.absoluteString, "https://opencode.ai/console/login")
        XCTAssertTrue(config.isAuthHost("auth.opencode.ai", "/"))
        XCTAssertTrue(config.isAuthHost("opencode.ai", "/auth/authorize"))
        XCTAssertTrue(config.isAuthHost("opencode.ai", "/console/login"))
        XCTAssertFalse(config.isAuthHost("opencode.ai", "/console"))

        func returns(_ string: String) throws -> Bool {
            try config.isReturnPage(XCTUnwrap(URL(string: string)))
        }
        XCTAssertTrue(try returns("https://opencode.ai/console"))
        XCTAssertTrue(try returns("https://opencode.ai/console/wrk_abc123"))
        XCTAssertTrue(try returns("https://www.opencode.ai/workspace/wrk_abc123/go"))
        XCTAssertTrue(try returns("https://opencode.ai/auth/callback"))
        XCTAssertFalse(try returns("https://opencode.ai/console/login"))
        XCTAssertFalse(try returns("https://opencode.ai/auth/authorize"))
        XCTAssertFalse(try returns("https://auth.opencode.ai/console"))
        XCTAssertFalse(try returns("https://opencode.ai/pricing"))
        XCTAssertFalse(try returns("https://example.com/console"))
    }

    func testSignInRemembersTheReturnPageWorkspace() throws {
        let auth = makeAuth(signedIn: false)
        let config = auth.signInConfig
        config.onReturned?(try XCTUnwrap(URL(string: "https://opencode.ai/console")))
        XCTAssertNil(auth.workspaceID)
        config.onReturned?(try XCTUnwrap(URL(string: "https://opencode.ai/console/wrk_abc123")))
        XCTAssertEqual(auth.workspaceID, "wrk_abc123")
    }

    func testAfterCaptureRecordsWorkspaceAndEmail() async {
        let auth = makeAuth(signedIn: true)
        var paths: [String] = []
        let config = auth.signInConfig { cookie in
            XCTAssertEqual(cookie, auth.cookieHeader())
            return OpenCodeConsoleClient { path, _ in
                await MainActor.run { paths.append(path) }
                if path == "/console/api/orgs" {
                    return Data(#"[{"id":"wrk_listed"}]"#.utf8)
                }
                return Data(#"{"user":{"id":"acc_1","email":"dev@example.com"}}"#.utf8)
            }
        }
        await config.afterCapture?()
        XCTAssertEqual(auth.workspaceID, "wrk_listed")
        XCTAssertEqual(auth.accountEmail, "dev@example.com")
        XCTAssertEqual(paths, ["/console/api/orgs", "/console/auth/session"])

        // A workspace from the redirect is kept without listing the account's.
        paths = []
        auth.saveWorkspaceID("wrk_redirect")
        await config.afterCapture?()
        XCTAssertEqual(auth.workspaceID, "wrk_redirect")
        XCTAssertEqual(paths, ["/console/auth/session"])
    }

    func testAfterCaptureWithoutACookieDoesNothing() async {
        let auth = makeAuth(signedIn: false)
        var made = 0
        let config = auth.signInConfig { _ in
            made += 1
            return OpenCodeConsoleClient { _, _ in Data() }
        }
        await config.afterCapture?()
        XCTAssertEqual(made, 0)
        XCTAssertNil(auth.workspaceID)
    }
}
