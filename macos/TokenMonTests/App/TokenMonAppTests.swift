import SwiftUI
@testable import TokenMon
import XCTest

/// The app's scene roots: each sign-in window shows its session's sheet, and
/// the menu bar root hosts the dropdown panel on the shared model.
@MainActor
final class TokenMonAppTests: XCTestCase {
    /// The sign-in window title for `provider`'s session. Grokbot signs in
    /// with Cursor's session; OpenRouter has no browser sign-in and falls back
    /// to Cursor's sheet like any other provider without its own.
    private func expectedTitle(_ provider: MonitorProvider, model: AppModel) -> String {
        switch provider {
        case .grok: model.grokAuth.signInConfig.title
        case .opencode: model.openCodeAuth.signInConfig.title
        case .claude: model.claudeAuth.signInConfig.title
        case .chatgpt: model.chatGPTAuth.signInConfig.title
        default: model.cursorAuth.signInConfig.title
        }
    }

    func testSignInWindowShowsTheSessionsSheet() {
        let model = AppModel()
        for provider in MonitorProvider.usageProviders {
            let tree = ViewTree(ProviderSignInRoot(model: model, provider: provider))
            XCTAssertTrue(tree.shows(expectedTitle(provider, model: model)), "\(provider)")
        }
        let grok = ViewTree(ProviderSignInRoot(model: model, provider: .grok))
        XCTAssertEqual(grok.first(ProviderSignInSheet<GrokAuthSession>.self)?.config.title, model.grokAuth.signInConfig.title)
        let grokbot = ViewTree(ProviderSignInRoot(model: model, provider: .grokbot))
        XCTAssertNotNil(grokbot.first(ProviderSignInSheet<CursorAuthSession>.self))
        XCTAssertNotEqual(model.cursorAuth.signInConfig.title, model.grokAuth.signInConfig.title)
    }

    /// Finishing the Grok sign-in refreshes Grok at once; signed out, that
    /// refresh reports the missing session.
    func testCompletedSignInRefreshesThatProvider() async throws {
        let model = AppModel()
        XCTAssertNil(model.grokPoller.lastError)
        let tree = ViewTree(ProviderSignInRoot(model: model, provider: .grok))
        tree.first(ProviderSignInSheet<GrokAuthSession>.self)?.onComplete()
        try await waitUntil { model.grokPoller.lastError != nil }
        XCTAssertEqual(model.grokPoller.lastError, ProviderError.notSignedIn(.grok).localizedDescription)
    }

    func testMenuBarRootHostsThePanelOnTheModel() {
        let model = AppModel()
        let root = MenuBarRoot(model: model)
        XCTAssertGreaterThan(ViewHarness.render(root, width: MenuBarPanelView.panelWidth).height, 0)
        let panel = ViewTree(root).first(MenuBarPanelView.self)
        XCTAssertIdentical(panel?.model, model)
        XCTAssertIdentical(panel?.settings, model.settings)
    }

    /// Polls `condition` on the main actor until it holds or `timeout` passes.
    private func waitUntil(
        timeout: TimeInterval = 2,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ condition: () -> Bool
    ) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition() {
            guard Date() < deadline else {
                XCTFail("condition not met within \(timeout)s", file: file, line: line)
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}
