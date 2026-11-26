import ServiceManagement
import SwiftUI
@testable import TokenMon
import XCTest

/// A login item whose status a test sets; registering is recorded, never real.
private final class FakeLoginItem: LoginItemService {
    var status: SMAppService.Status
    init(status: SMAppService.Status = .notRegistered) { self.status = status }
    func register() throws { status = .enabled }
    func unregister() throws { status = .notRegistered }
}

/// A poller whose refresh time and error a test sets.
@MainActor
@Observable
private final class FakePoller: ProviderUsagePoller {
    var menuIsOpen = false
    var lastRefreshedAt: Date?
    var lastError: String?
    var headlineUsedPercent: Double? { nil }
    var headlineResetsAt: Date? { nil }
    var alertAccount: String? { nil }
    @ObservationIgnored let pollingLoop = PollingLoop(interval: { nil }, refresh: {})
    func clearSnapshot() {}
}

/// The Settings window: each tab shows its settings and its controls change
/// them. Sessions use in-memory stores; nothing is registered with macOS,
/// opened, or fetched.
@MainActor
final class PreferencesViewTests: XCTestCase {
    private var suiteName: String!
    private var defaults: UserDefaults!
    private var dir: URL!
    private var opened: [AppWindow] = []

    override func setUp() async throws {
        suiteName = "PreferencesView-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        opened = []
    }

    override func tearDown() async throws {
        defaults.removePersistentDomain(forName: suiteName)
        try? FileManager.default.removeItem(at: dir)
    }

    private func makeSettings(loginStatus: SMAppService.Status = .notRegistered) -> AppSettings {
        AppSettings(defaults: defaults, loginItem: FakeLoginItem(status: loginStatus))
    }

    /// A checker without a version: a manual check explains and sends nothing.
    private func makeChecker(_ settings: AppSettings) -> UpdateChecker {
        UpdateChecker(settings: settings, currentVersion: nil, openURL: { _ in })
    }

    private func general(_ settings: AppSettings, model: AppModel? = nil) -> GeneralSettingsTab {
        GeneralSettingsTab(model: model ?? AppModel(), settings: settings, updateChecker: makeChecker(settings))
    }

    /// A session that signs in with a cookie header and touches no shared jar.
    private func makeSession() -> ProviderAuthSession {
        ProviderAuthSession(
            config: ProviderAuthConfig(
                storeFilenamePrefix: "auth_",
                logCategory: "TestAuth",
                extraStoreKeys: [],
                signOutHosts: [],
                capturePolicy: WebKitCookieCapture.Policy(isDomain: { _ in false }, failureMessage: "No session found."),
                isDomain: { _ in false }
            ),
            store: InMemoryCredentialStore()
        )
    }

    // MARK: - Window

    func testWindowHasTheFourTabs() {
        let model = AppModel()
        let settings = makeSettings()
        let view = PreferencesView(
            model: model,
            settings: settings,
            history: GrokHistoryStore(inMemory: true),
            updateChecker: makeChecker(settings),
            openWindow: { [weak self] in self?.opened.append($0) }
        )
        ViewHarness.render(view, width: 520)
        let tree = ViewTree(view)
        XCTAssertNotNil(tree.first(GeneralSettingsTab.self))
        XCTAssertNotNil(tree.first(ProvidersSettingsTab.self))
        XCTAssertNotNil(tree.first(AccountsSettingsTab.self))
        XCTAssertNotNil(tree.first(DataSettingsTab.self))
        for tab in SettingsTab.allCases {
            XCTAssertTrue(tree.shows(tab.displayName), tab.displayName)
        }
    }

    // MARK: - General

    func testGeneralTabShowsStartupUpdatesAndRefresh() {
        let settings = makeSettings()
        let tab = general(settings)
        ViewHarness.renderBothAppearances(tab, width: 520)
        let tree = ViewTree(tab)
        XCTAssertTrue(tree.shows("Launch at Login"))
        XCTAssertFalse(tree.shows("Waiting for approval"))
        XCTAssertTrue(tree.shows("Check for Updates"))
        XCTAssertTrue(tree.shows("While menu is open"))
        XCTAssertTrue(tree.shows("While idle"))
        XCTAssertTrue(tree.shows("Quit TokenMon"))
        XCTAssertFalse(tree.shows("Alert at"))
    }

    func testLoginItemAwaitingApprovalPointsToSystemSettings() {
        let tree = ViewTree(general(makeSettings(loginStatus: .requiresApproval)))
        XCTAssertTrue(tree.shows("Waiting for approval in System Settings › General › Login Items."))
        XCTAssertTrue(tree.shows("Open Login Items…"))
    }

    func testUpdateButtonChecksAndShowsTheResult() async {
        let settings = makeSettings()
        let checker = makeChecker(settings)
        let tab = GeneralSettingsTab(model: AppModel(), settings: settings, updateChecker: checker)
        ViewTree(tab).press("Check for Updates")
        await waitUntil { checker.statusMessage != nil }
        let tree = ViewTree(tab)
        XCTAssertTrue(tree.shows("This build has no version number to compare."))
        ViewHarness.render(tab, width: 520)
    }

    func testAlertsListEveryVisibleProviderWhenOn() {
        let settings = makeSettings()
        settings.thresholdEnabled = true
        let tab = general(settings)
        ViewHarness.render(tab, width: 520)
        let tree = ViewTree(tab)
        XCTAssertTrue(tree.shows("Alert at"))
        XCTAssertTrue(tree.shows("Providers the alert watches keep refreshing in the background."))
        for provider in settings.visibleUsageProviders {
            XCTAssertEqual(tree.toggle(provider.displayName)?.wrappedValue, true, provider.displayName)
        }

        let claude = tree.toggle(MonitorProvider.claude.displayName)
        claude?.wrappedValue = false
        XCTAssertFalse(settings.alertProviders.contains(.claude))
        claude?.wrappedValue = true
        XCTAssertTrue(settings.alertProviders.contains(.claude))
    }

    /// Overview lists every provider's last refresh; a provider tab lists its own.
    func testRefreshStatusFollowsTheSelectedProvider() {
        let model = AppModel()
        let settings = makeSettings()
        settings.selectedProvider = .overview
        let overview = general(settings, model: model)
        XCTAssertEqual(overview.refreshRows.map(\.provider), model.providers.all.map(\.provider))
        XCTAssertEqual(overview.refreshRows.map(\.label), model.providers.all.map(\.provider.displayName))
        ViewHarness.render(overview, width: 520)

        settings.selectedProvider = .claude
        let claude = general(settings, model: model)
        XCTAssertEqual(claude.refreshRows.map(\.provider), [.claude])
        XCTAssertEqual(claude.refreshRows.map(\.label), [nil])
        XCTAssertIdentical(claude.refreshRows.first?.poller, model.claudePoller)
        ViewHarness.render(claude, width: 520)
    }

    // MARK: - Refresh status

    func testRefreshStatusShowsTheTimeAndError() {
        let poller = FakePoller()
        XCTAssertTrue(ViewTree(PollerRefreshStatus(poller: poller, label: nil)).texts.isEmpty)

        poller.lastRefreshedAt = Date(timeIntervalSince1970: 1_700_000_000)
        poller.lastError = "Session expired"
        let own = PollerRefreshStatus(poller: poller, label: nil)
        ViewHarness.render(own)
        var tree = ViewTree(own)
        XCTAssertTrue(tree.shows("Last refresh: "))
        XCTAssertTrue(tree.shows("Session expired"))

        tree = ViewTree(PollerRefreshStatus(poller: poller, label: "Grok"))
        XCTAssertTrue(tree.texts.contains("Grok"), "the row names its provider")
        XCTAssertFalse(tree.shows("Session expired"), "Overview rows show only the time")
    }

    // MARK: - Providers

    func testProvidersTabTogglesProvidersAndGraphs() {
        let settings = makeSettings()
        let tab = ProvidersSettingsTab(settings: settings)
        ViewHarness.renderBothAppearances(tab, width: 520)
        let tree = ViewTree(tab)
        XCTAssertTrue(tree.shows("Choose which graphs stay pinned in the menu bar."))
        XCTAssertTrue(tree.shows("Grok categories"))

        let claude = tree.toggle(MonitorProvider.claude.displayName)
        XCTAssertEqual(claude?.wrappedValue, true)
        claude?.wrappedValue = false
        XCTAssertFalse(settings.enabledProviderIDs.contains(.claude))
        claude?.wrappedValue = true
        XCTAssertTrue(settings.enabledProviderIDs.contains(.claude))

        let graph = tree.toggle("bar graph")
        let first = MonitorProvider.menuBarGraphProviders.first!
        let before = settings.showsMenuBarGraph(first)
        graph?.wrappedValue = !before
        XCTAssertEqual(settings.showsMenuBarGraph(first), !before)
    }

    func testSelectedProviderOnlyHidesTheGraphToggles() {
        let settings = makeSettings()
        settings.showSelectedProviderInMenuBar = true
        let tree = ViewTree(ProvidersSettingsTab(settings: settings))
        XCTAssertTrue(tree.shows("The menu bar shows only the active provider's icon, percentage, and usage bar."))
        XCTAssertFalse(tree.toggles.contains { $0.label.contains { $0.contains("bar graph") } })
    }

    /// Dragging a row over another moves it there; dropping ends the drag.
    func testDraggingARowReordersProviders() throws {
        let settings = makeSettings()
        let order = settings.orderedUsageProviders
        let moving = try XCTUnwrap(order.last)
        let target = try XCTUnwrap(order.first)
        var dragging: MonitorProvider? = moving
        let binding = Binding(get: { dragging }, set: { dragging = $0 })

        ProviderReorderDropDelegate(target: moving, dragging: binding, move: settings.moveProvider).moveDraggedProvider()
        XCTAssertEqual(settings.orderedUsageProviders, order, "a row over itself stays put")

        let delegate = ProviderReorderDropDelegate(target: target, dragging: binding, move: settings.moveProvider)
        delegate.moveDraggedProvider()
        XCTAssertEqual(settings.orderedUsageProviders.first, moving)

        XCTAssertTrue(delegate.finishDrag())
        XCTAssertNil(dragging)
        let afterDrop = settings.orderedUsageProviders
        delegate.moveDraggedProvider()
        XCTAssertEqual(settings.orderedUsageProviders, afterDrop, "nothing moves without a drag")
    }

    // MARK: - Accounts

    func testAccountsTabListsEveryAccountAndOpensItsSignIn() {
        let model = AppModel()
        let tab = AccountsSettingsTab(model: model, openWindow: { [weak self] in self?.opened.append($0) })
        ViewHarness.render(tab, width: 520)
        let tree = ViewTree(tab)
        XCTAssertTrue(tree.shows(AccountsPrivacyNote.text))
        XCTAssertTrue(tree.shows("Grokbot uses your Cursor account."))
        XCTAssertTrue(tree.shows("Not connected"))

        // A sectioned form repeats its rows in the walk; each account appears once in order.
        var seen = Set<MonitorProvider>()
        let rows = tree.all(ProviderAccountRow.self).filter { seen.insert($0.provider).inserted }
        XCTAssertEqual(rows.map(\.provider), [.grok, .claude, .chatgpt, .cursor, .opencode])
        for row in rows {
            row.openSignIn()
        }
        XCTAssertEqual(opened, [.signIn(.grok), .signIn(.claude), .signIn(.chatgpt), .signIn(.cursor), .signIn(.opencode)])
    }

    func testSignedOutRowOffersSignInAndShowsTheLastError() async {
        let auth = makeSession()
        var openedSignIn = 0
        let row = ProviderAccountRow(provider: .claude, auth: auth, signInTitle: "Sign In…") { openedSignIn += 1 }
        var tree = ViewTree(row)
        XCTAssertTrue(tree.shows("Not signed in"))
        tree.press("Sign In…")
        XCTAssertEqual(openedSignIn, 1)

        _ = await auth.captureCookiesFromWebKit()
        tree = ViewTree(row)
        XCTAssertTrue(tree.shows("No session found."))
        ViewHarness.render(row)
    }

    func testSignedInRowShowsTheAccountAndSignsOut() {
        let auth = makeSession()
        auth.save(cookieHeader: "sid=1")
        var openedSignIn = 0
        let row = ProviderAccountRow(provider: .cursor, auth: auth, signInTitle: "Sign In…") { openedSignIn += 1 }
        ViewHarness.renderBothAppearances(row)
        let tree = ViewTree(row)
        XCTAssertTrue(tree.shows("Signed in"))
        tree.press("Re-authenticate…")
        XCTAssertEqual(openedSignIn, 1)
        tree.press("Sign Out")
        XCTAssertFalse(auth.isSignedIn)
    }

    // MARK: - OpenRouter

    private func openRouter() -> (OpenRouterAuthSession, OpenRouterUsagePoller, () -> Int) {
        let auth = OpenRouterAuthSession(store: InMemoryCredentialStore())
        var fetches = 0
        let poller = OpenRouterUsagePoller(settings: makeSettings(), auth: auth) { _ in
            fetches += 1
            throw CancellationError()
        }
        return (auth, poller, { fetches })
    }

    func testOpenRouterSavesAPastedKeyAndRefreshes() async {
        let (auth, poller, fetches) = openRouter()
        let empty = OpenRouterAccountSection(auth: auth, poller: poller)
        ViewHarness.render(empty, width: 520)
        XCTAssertTrue(ViewTree(empty).shows("Save API Key"))

        let typed = OpenRouterAccountSection(auth: auth, poller: poller, keyDraft: "  sk-or-v1-abc  ")
        ViewTree(typed).press("Save API Key")
        XCTAssertTrue(auth.isSignedIn)
        await waitUntil { fetches() == 1 }
    }

    func testOpenRouterShowsARejectedKey() {
        let (auth, poller, fetches) = openRouter()
        let typed = OpenRouterAccountSection(auth: auth, poller: poller, keyDraft: "not-a-key")
        ViewTree(typed).press("Save API Key")
        XCTAssertFalse(auth.isSignedIn)
        XCTAssertEqual(fetches(), 0)
        XCTAssertTrue(ViewTree(typed).shows("That doesn't look like an OpenRouter key"))
    }

    func testOpenRouterConnectedSignsOut() {
        let (auth, poller, _) = openRouter()
        XCTAssertTrue(auth.saveAPIKey("sk-or-v1-abc"))
        let section = OpenRouterAccountSection(auth: auth, poller: poller)
        ViewHarness.render(section, width: 520)
        let tree = ViewTree(section)
        XCTAssertTrue(tree.shows("Connected with"))
        XCTAssertTrue(tree.shows("OpenRouter API key"))
        tree.press("Sign Out")
        XCTAssertFalse(auth.isSignedIn)
    }

    // MARK: - Data

    func testDataTabTogglesCategoriesAndClearsHistory() {
        let settings = makeSettings()
        let history = GrokHistoryStore(inMemory: true)
        history.append(WeeklyUsageSnapshot(usedPercent: 40, remainingPercent: 60))
        let tab = DataSettingsTab(settings: settings, history: history)
        ViewHarness.renderBothAppearances(tab, width: 520)
        let tree = ViewTree(tab)
        XCTAssertTrue(tree.shows("SuperGrok Categories"))
        XCTAssertFalse(tree.shows("History storage is unavailable"))

        let chat = tree.toggle(ProductCatalog.displayName(for: "chat"))
        chat?.wrappedValue = false
        XCTAssertFalse(settings.visibleProductIDs.contains("chat"))
        chat?.wrappedValue = true
        XCTAssertTrue(settings.visibleProductIDs.contains("chat"))

        XCTAssertFalse(history.allSnapshots().isEmpty)
        tree.press("Clear Local History")
        XCTAssertTrue(history.allSnapshots().isEmpty)
    }

    func testExportButtonsSaveCSVAndJSON() throws {
        let history = GrokHistoryStore(inMemory: true)
        history.append(WeeklyUsageSnapshot(usedPercent: 40, remainingPercent: 60))
        var saved: [DataSettingsTab.ExportFile] = []
        let tab = DataSettingsTab(settings: makeSettings(), history: history) { saved.append($0) }
        let tree = ViewTree(tab)
        tree.press("CSV…")
        tree.press("JSON…")

        XCTAssertEqual(saved.map(\.filename), ["grok-usage.csv", "grok-usage.json"])
        XCTAssertEqual(saved.map(\.contentType), [.commaSeparatedText, .json])
        let csv = try XCTUnwrap(String(data: saved[0].data, encoding: .utf8))
        XCTAssertTrue(csv.hasPrefix("fetchedAt,usedPercent"))
        XCTAssertNoThrow(try JSONSerialization.jsonObject(with: saved[1].data))
    }

    func testExportFailureAndBrokenStoreAreShown() throws {
        let store = FileBackedStringStore(directory: dir, filenamePrefix: "grok_")
        XCTAssertTrue(store.set("not json", forKey: "history"))
        let history = GrokHistoryStore(store: store, legacyStoreURL: nil)
        var attempts = 0
        let tab = DataSettingsTab(settings: makeSettings(), history: history, exportError: "The disk is full.") { _ in
            attempts += 1
            throw CocoaError(.fileWriteOutOfSpace)
        }
        ViewHarness.render(tab, width: 520)
        let tree = ViewTree(tab)
        XCTAssertTrue(tree.shows("The disk is full."))
        XCTAssertEqual(history.storeFailed, tree.shows("History storage is unavailable"))
        tree.press("CSV…")
        XCTAssertEqual(attempts, 1)
    }

    private func waitUntil(timeout: TimeInterval = 5, _ condition: () -> Bool) async {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition(), Date() < deadline {
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertTrue(condition(), "timed out")
    }
}
