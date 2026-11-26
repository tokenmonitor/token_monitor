import SwiftUI
@testable import TokenMon
import XCTest

/// The provider registry starts every poller, the switcher's tabs select
/// their provider, and the logos and header fall back sensibly.
@MainActor
final class ProviderRegistryViewTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var defaults: UserDefaults!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "ProviderRegistry-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suiteName)
    }

    func testStartAllStartsEveryRegisteredPoller() async {
        let settings = AppSettings(defaults: defaults)
        settings.selectedProvider = .openrouter
        let auth = OpenRouterAuthSession(directory: dir, store: InMemoryCredentialStore())
        XCTAssertTrue(auth.saveAPIKey("sk-or-v1-test"))
        let fetched = expectation(description: "start polls")
        fetched.assertForOverFulfill = false
        let poller = OpenRouterUsagePoller(settings: settings, auth: auth) { _ in
            fetched.fulfill()
            throw URLError(.timedOut)
        }
        let registry = ProviderRegistry([ProviderRegistry.Entry(provider: .openrouter, poller: poller, auth: auth)])
        XCTAssertEqual(registry.all.map(\.provider), [.openrouter])
        registry.startAll()
        await fulfillment(of: [fetched], timeout: 5)
        poller.stop()
    }

    func testSwitcherTabSelectsItsProvider() {
        var selection = MonitorProvider.overview
        let binding = Binding(get: { selection }, set: { selection = $0 })
        let switcher = ProviderSwitcherView(providers: [.overview, .claude], selection: binding)
        ViewHarness.renderBothAppearances(switcher)

        let claudeTab = switcher.tabButton(.claude)
        let tree = ViewTree(claudeTab)
        XCTAssertTrue(tree.shows("Claude"))
        tree.press("Claude")
        XCTAssertEqual(selection, .claude)

        let overviewTab = ViewTree(switcher.tabButton(.overview))
        XCTAssertTrue(overviewTab.shows("All"))
        overviewTab.press("All")
        XCTAssertEqual(selection, .overview)
    }

    func testOverviewFallsBackToTheGrokMarkAndASymbolHeader() {
        XCTAssertIdentical(ProviderLogo.image(for: .overview), ProviderLogo.grok)
        for provider in MonitorProvider.usageProviders {
            XCTAssertTrue(ProviderLogo.image(for: provider).isTemplate, provider.rawValue)
        }
        XCTAssertTrue(ProviderLogo.tokenmon.isTemplate)

        let overview = ProviderHeaderLabel(provider: .overview, title: "Overview")
        let logo = ProviderHeaderLabel(provider: .claude, title: "Claude")
        ViewHarness.render(overview)
        ViewHarness.render(logo)
        XCTAssertTrue(ViewTree(overview).shows("square.grid.2x2"))
        XCTAssertFalse(ViewTree(logo).shows("square.grid.2x2"))
    }

    func testHeaderCogOpensPreferencesFromTheEnvironment() {
        var environment = EnvironmentValues()
        // The default action does nothing until the panel root injects one.
        environment.openPreferences()
        var opened = 0
        environment.openPreferences = { opened += 1 }
        environment.openPreferences()
        XCTAssertEqual(opened, 1)

        let cog = SettingsCogButton { opened += 1 }
        ViewHarness.render(cog)
        ViewTree(cog).press("gearshape")
        XCTAssertEqual(opened, 2)
    }

    func testProviderIDIsItsRawValue() {
        XCTAssertEqual(MonitorProvider.allCases.map(\.id), MonitorProvider.allCases.map(\.rawValue))
        XCTAssertEqual(MonitorProvider.grokbot.id, "grokbot")
    }
}
