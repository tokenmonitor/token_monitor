import SwiftUI
@testable import TokenMon
import XCTest

/// The dropdown shows the selected provider's tab and wires each tab's
/// sign-in, settings and section state to the app.
@MainActor
final class MenuBarPanelViewTests: XCTestCase {
    private final class Windows {
        var opened: [AppWindow] = []
    }

    private func panel(_ model: AppModel, _ windows: Windows) -> MenuBarPanelView {
        MenuBarPanelView(model: model, settings: model.settings) { windows.opened.append($0) }
    }

    /// Selects `provider`, renders the panel, and returns its view tree.
    private func tree(_ model: AppModel, _ windows: Windows, selecting provider: MonitorProvider) -> ViewTree {
        model.settings.selectedProvider = provider
        let view = panel(model, windows)
        XCTAssertGreaterThan(ViewHarness.render(view, width: MenuBarPanelView.panelWidth).height, 0, provider.rawValue)
        return ViewTree(view)
    }

    /// Each provider tab opens the sign-in window for its own session;
    /// Grokbot signs in through Cursor.
    func testEachTabOpensItsOwnSignIn() {
        let model = AppModel()
        let windows = Windows()
        tree(model, windows, selecting: .grok).first(GrokPanelView.self)?.openSignIn()
        tree(model, windows, selecting: .opencode).first(OpenCodePanelView.self)?.openSignIn()
        tree(model, windows, selecting: .cursor).first(CursorPanelView.self)?.openSignIn()
        tree(model, windows, selecting: .claude).first(ClaudePanelView.self)?.openSignIn()
        tree(model, windows, selecting: .chatgpt).first(ChatGPTPanelView.self)?.openSignIn()
        tree(model, windows, selecting: .grokbot).first(GrokbotPanelView.self)?.openSignIn()
        XCTAssertEqual(
            windows.opened,
            [.signIn(.grok), .signIn(.opencode), .signIn(.cursor), .signIn(.claude), .signIn(.chatgpt), .signIn(.cursor)]
        )
    }

    func testOpenRouterTabShowsItsPanel() {
        let model = AppModel()
        let tree = tree(model, Windows(), selecting: .openrouter)
        XCTAssertNotNil(tree.first(OpenRouterPanelView.self))
        XCTAssertNil(tree.all(GrokPanelView.self).first)
    }

    /// The Overview tab opens sign-in and settings, switches to OpenRouter,
    /// and reads the day's series from the model.
    func testOverviewTabWiresItsActions() throws {
        let model = AppModel()
        let windows = Windows()
        let overview = try XCTUnwrap(tree(model, windows, selecting: .overview).first(OverviewPanelView.self))
        overview.openSignIn(.claude)
        overview.openPreferences()
        XCTAssertEqual(windows.opened, [.signIn(.claude), .preferences])
        overview.selectOpenRouter()
        XCTAssertEqual(model.settings.selectedProvider, .openrouter)
        XCTAssertEqual(overview.todaySeries(Date()), model.overviewSeries(on: Date()))
    }

    /// The models sections' open state is read from and saved to settings.
    func testModelsSectionsRememberTheirState() throws {
        let model = AppModel()
        let claude = try XCTUnwrap(tree(model, Windows(), selecting: .claude).first(ClaudePanelView.self))
        let wasOpen = model.settings.isExpanded(.claudeModels)
        XCTAssertEqual(claude.modelsExpanded, wasOpen)
        claude.modelsExpanded = !wasOpen
        XCTAssertEqual(model.settings.isExpanded(.claudeModels), !wasOpen)

        let openCode = try XCTUnwrap(tree(model, Windows(), selecting: .opencode).first(OpenCodePanelView.self))
        let codeWasOpen = model.settings.isExpanded(.openCodeModels)
        openCode.modelsExpanded = !codeWasOpen
        XCTAssertEqual(model.settings.isExpanded(.openCodeModels), !codeWasOpen)
    }

    /// The Settings cog in every provider header opens Settings: the panel
    /// injects that action into the environment.
    func testSettingsCogOpensPreferences() throws {
        let model = AppModel()
        let windows = Windows()
        model.settings.selectedProvider = .grok
        let action = try XCTUnwrap(Self.environmentAction(in: panel(model, windows).body))
        action()
        XCTAssertEqual(windows.opened, [.preferences])
    }

    /// The `() -> Void` an environment-writing modifier in `value` injects.
    private static func environmentAction(in value: Any, depth: Int = 12) -> (() -> Void)? {
        guard depth > 0 else { return nil }
        let isWriter = String(describing: type(of: value)).hasPrefix("_EnvironmentKeyWritingModifier")
        for child in Mirror(reflecting: value).children {
            if isWriter, let action = child.value as? () -> Void {
                return action
            }
            if let found = environmentAction(in: child.value, depth: depth - 1) {
                return found
            }
        }
        return nil
    }

    func testPanelHasItsFixedWidth() {
        let model = AppModel()
        model.settings.selectedProvider = .overview
        XCTAssertEqual(ViewHarness.idealSize(panel(model, Windows())).width, MenuBarPanelView.panelWidth)
    }
}
