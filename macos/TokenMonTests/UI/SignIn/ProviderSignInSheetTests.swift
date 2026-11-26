import SwiftUI
@testable import TokenMon
import XCTest

/// The sign-in window shows the flow's state, wires its buttons, and its
/// browser chrome follows the web view. The browser starts on about:blank;
/// nothing loads from the network.
@MainActor
final class ProviderSignInSheetTests: XCTestCase {
    private func sheet(
        _ auth: FakeSignInAuth,
        config: ProviderSignInConfig = .fixture(),
        flow: ProviderSignInFlow? = nil,
        onComplete: @escaping () -> Void = {}
    ) -> ProviderSignInSheet<FakeSignInAuth> {
        ProviderSignInSheet(auth: auth, config: config, flow: flow, onComplete: onComplete)
    }

    private func flow(_ auth: FakeSignInAuth, config: ProviderSignInConfig = .fixture(), onComplete: @escaping () -> Void = {}) -> ProviderSignInFlow {
        ProviderSignInFlow(auth: auth, config: config, sleep: { _ in }, onComplete: onComplete)
    }

    func testSigningInShowsTheProviderPageAndActions() {
        let view = sheet(FakeSignInAuth())
        let size = ViewHarness.renderBothAppearances(view, width: 880)
        XCTAssertGreaterThan(size.height, 100)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Sign in to Example"))
        XCTAssertTrue(tree.shows("Sign in with your Example account."))
        XCTAssertTrue(tree.shows("Cancel"))
        XCTAssertTrue(tree.shows("Finish Sign-In"))
        XCTAssertNotNil(tree.first(SignInNavigationBar.self))
        XCTAssertNotNil(tree.first(ProviderSignInWebView.self))
        tree.press("Cancel")
    }

    func testFinishButtonCapturesAndCompletes() async {
        let auth = FakeSignInAuth(captureResults: [true], accountEmail: "a@b.com")
        var completed = 0
        let flow = flow(auth) { completed += 1 }
        let view = sheet(auth, flow: flow)
        ViewTree(view).press("Finish Sign-In")
        await waitUntil { flow.isSignedIn }
        XCTAssertEqual(completed, 1)

        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Signed in as a@b.com"))
        XCTAssertTrue(tree.shows("Signed In"))
        ViewHarness.render(view, width: 880)
    }

    func testFailedCaptureShowsTheErrorAndTryAgain() async {
        let auth = FakeSignInAuth(captureResults: [false], lastAuthError: "No Example session cookie found.")
        let flow = flow(auth)
        await flow.finish()
        let tree = ViewTree(sheet(auth, flow: flow))
        XCTAssertTrue(tree.shows("No Example session cookie found."))
        XCTAssertTrue(tree.shows("Try Again"))
    }

    /// Reaching the return page runs the provider hook, then captures.
    func testReturnPageRunsTheHookAndCaptures() async throws {
        let auth = FakeSignInAuth(captureResults: [true])
        var returnedURLs: [URL] = []
        let config = ProviderSignInConfig.fixture(onReturned: { returnedURLs.append($0) })
        let flow = flow(auth, config: config)
        let view = sheet(auth, config: config, flow: flow)
        let url = try XCTUnwrap(URL(string: "https://example.com/home"))
        view.returned(url)
        XCTAssertEqual(returnedURLs, [url])
        await waitUntil { flow.isSignedIn }
        XCTAssertEqual(auth.captureCount, 1)
    }

    /// The web view's return callback reaches the sheet.
    func testWebViewIsWiredToTheSheet() async throws {
        let auth = FakeSignInAuth(captureResults: [true])
        let flow = flow(auth)
        let web = try XCTUnwrap(ViewTree(sheet(auth, flow: flow)).first(ProviderSignInWebView.self))
        XCTAssertTrue(web.isAuthHost("auth.example.com", "/"))
        XCTAssertTrue(web.isReturnPage(try XCTUnwrap(URL(string: "https://example.com/"))))
        XCTAssertEqual(web.startURL.absoluteString, "about:blank")
        XCTAssertIdentical(web.controller, flow.browser)
        web.onAuthHostSeen()
        web.onReturned(try XCTUnwrap(URL(string: "https://example.com/")))
        await waitUntil { flow.isSignedIn }
    }

    // MARK: - Navigation bar

    private func controller(back: Bool = false, forward: Bool = false, loading: Bool = false, url: String = "", popups: Int = 0) -> SignInBrowserController {
        let controller = SignInBrowserController()
        controller.apply(canGoBack: back, canGoForward: forward, isLoading: loading, currentURL: url, popupDepth: popups)
        return controller
    }

    func testBarShowsTheHostAndPathOfThePage() {
        let bar = SignInNavigationBar(browser: controller(back: true, url: "https://example.com/login?next=1"))
        ViewHarness.render(bar, width: 880)
        let tree = ViewTree(bar)
        XCTAssertTrue(tree.shows("example.com/login"))
        XCTAssertFalse(tree.shows("Close popup"))
        XCTAssertTrue(tree.shows("Reload"))
        XCTAssertTrue(tree.shows("Back"))
        XCTAssertTrue(tree.shows("Forward"))
    }

    func testBarShowsARawURLWithoutAHost() {
        let tree = ViewTree(SignInNavigationBar(browser: controller(url: "about:blank")))
        XCTAssertTrue(tree.shows("about:blank"))
    }

    func testBarOffersClosingAPopupAndStopWhileLoading() {
        let bar = SignInNavigationBar(browser: controller(loading: true, url: "https://auth.example.com/", popups: 1))
        ViewHarness.renderBothAppearances(bar, width: 880)
        let tree = ViewTree(bar)
        XCTAssertTrue(tree.shows("Sign-in popup"))
        XCTAssertTrue(tree.shows("Close popup"))
        XCTAssertTrue(tree.shows("Back to the sign-in page"))
        XCTAssertTrue(tree.shows("Stop"))
    }

    /// The bar's buttons drive the attached browser: Close popup removes the
    /// popup, and Back with no history does the same.
    func testBarButtonsDriveTheBrowser() {
        let controller = SignInBrowserController()
        let browser = SignInBrowserViewTests.makeBrowser()
        controller.attach(browser)
        let bar = SignInNavigationBar(browser: controller)

        _ = SignInBrowserViewTests.openPopup(in: browser)
        XCTAssertEqual(controller.popupDepth, 1)
        ViewTree(bar).press("Close popup")
        XCTAssertEqual(controller.popupDepth, 0)

        _ = SignInBrowserViewTests.openPopup(in: browser)
        ViewTree(bar).press("Back")
        XCTAssertEqual(controller.popupDepth, 0)

        ViewTree(bar).press("Forward")
        ViewTree(bar).press(controller.isLoading ? "Stop" : "Reload")
        XCTAssertEqual(controller.popupDepth, 0)
    }

    // MARK: - Hosting

    /// Hosting the web view builds a browser wired to the sheet's controller
    /// and keeps its callbacks current.
    func testHostedWebViewAttachesItsController() {
        let controller = SignInBrowserController()
        var returned: [URL] = []
        let web = ProviderSignInWebView(
            startURL: URL(string: "about:blank")!,
            dataStore: .nonPersistent(),
            controller: controller,
            isAuthHost: { host, _ in host == "auth.example.com" },
            isReturnPage: { $0.host == "example.com" },
            onAuthHostSeen: {},
            onReturned: { returned.append($0) }
        )
        let host = NSHostingView(rootView: web.frame(width: 400, height: 300))
        host.frame = NSRect(x: 0, y: 0, width: 400, height: 300)
        host.layoutSubtreeIfNeeded()
        let browser = host.subviews(of: SignInBrowserView.self).first
        XCTAssertNotNil(browser)
        browser?.handleFinished(URL(string: "https://auth.example.com/")!, isPopup: false)
        browser?.handleFinished(URL(string: "https://example.com/")!, isPopup: false)
        XCTAssertEqual(returned.map(\.absoluteString), ["https://example.com/"])

        // A re-render hands the browser the new callbacks.
        var updated: [URL] = []
        var replacement = web
        replacement.onReturned = { updated.append($0) }
        host.rootView = replacement.frame(width: 400, height: 300)
        host.layoutSubtreeIfNeeded()
        browser?.rearmReturn()
        browser?.handleFinished(URL(string: "https://example.com/")!, isPopup: false)
        XCTAssertEqual(updated.map(\.absoluteString), ["https://example.com/"])
        XCTAssertEqual(returned.count, 1)
    }

    private func waitUntil(timeout: TimeInterval = 5, _ condition: () -> Bool) async {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition(), Date() < deadline {
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertTrue(condition(), "timed out")
    }
}

private extension NSView {
    /// Every descendant view of type `T`, depth first.
    func subviews<T: NSView>(of _: T.Type) -> [T] {
        subviews.flatMap { ($0 as? T).map { [$0] } ?? $0.subviews(of: T.self) }
    }
}
