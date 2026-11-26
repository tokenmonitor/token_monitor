@testable import TokenMon
import WebKit
import XCTest

/// Stand-ins WebKit hands the UI delegate when a page calls `window.open`.
private final class FakeNavigationAction: WKNavigationAction {}
private final class FakeWindowFeatures: WKWindowFeatures {}

/// The AppKit sign-in browser: OAuth popups stack over the provider page,
/// Back closes a popup without history, and the return callback fires once
/// per sign-in, only after the main page is back with no popup open.
///
/// Navigations are reported by calling the delegate entry points directly;
/// the only page ever loaded is about:blank.
@MainActor
final class SignInBrowserViewTests: XCTestCase {
    static let authURL = URL(string: "https://auth.example.com/login")!
    static let returnURL = URL(string: "https://example.com/home")!

    /// A browser on about:blank whose return page is example.com (or
    /// about:blank itself when `blankIsReturnPage`).
    static func makeBrowser(
        blankIsReturnPage: Bool = false,
        onAuthHostSeen: @escaping () -> Void = {},
        onReturned: @escaping (URL) -> Void = { _ in }
    ) -> SignInBrowserView {
        SignInBrowserView(
            startURL: URL(string: "about:blank")!,
            dataStore: .nonPersistent(),
            isAuthHost: { host, _ in host == "auth.example.com" },
            isReturnPage: { url in url.host == "example.com" || (blankIsReturnPage && url.absoluteString == "about:blank") },
            onAuthHostSeen: onAuthHostSeen,
            onReturned: onReturned
        )
    }

    /// Opens a popup the way WebKit does for `window.open`.
    static func openPopup(in browser: SignInBrowserView) -> WKWebView? {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        return browser.webView(
            browser.mainWebView,
            createWebViewWith: configuration,
            for: FakeNavigationAction(),
            windowFeatures: FakeWindowFeatures()
        )
    }

    private var returned: [URL] = []
    private var authSeen = 0

    override func setUp() async throws {
        returned = []
        authSeen = 0
    }

    private func makeBrowser(blankIsReturnPage: Bool = false) -> (SignInBrowserView, SignInBrowserController) {
        let browser = Self.makeBrowser(
            blankIsReturnPage: blankIsReturnPage,
            onAuthHostSeen: { [weak self] in self?.authSeen += 1 },
            onReturned: { [weak self] in self?.returned.append($0) }
        )
        let controller = SignInBrowserController()
        controller.attach(browser)
        return (browser, controller)
    }

    // MARK: - Popups

    func testPopupStacksOverThePageAndBackClosesIt() throws {
        let (browser, controller) = makeBrowser()
        browser.frame = NSRect(x: 0, y: 0, width: 600, height: 400)
        let popup = try XCTUnwrap(Self.openPopup(in: browser))
        XCTAssertIdentical(popup.navigationDelegate, browser)
        XCTAssertIdentical(popup.uiDelegate, browser)
        XCTAssertTrue(browser.subviews.contains(popup))
        XCTAssertEqual(controller.popupDepth, 1)
        XCTAssertTrue(controller.backIsEnabled)

        browser.layout()
        XCTAssertEqual(popup.frame, browser.bounds)
        XCTAssertEqual(browser.mainWebView.frame, browser.bounds)

        controller.goBack()
        XCTAssertEqual(controller.popupDepth, 0)
        XCTAssertFalse(browser.subviews.contains(popup))
        XCTAssertNil(popup.navigationDelegate)
    }

    func testPopupClosingItselfIsDismissed() throws {
        let (browser, controller) = makeBrowser()
        let first = try XCTUnwrap(Self.openPopup(in: browser))
        let second = try XCTUnwrap(Self.openPopup(in: browser))
        XCTAssertEqual(controller.popupDepth, 2)

        browser.webViewDidClose(first)
        XCTAssertEqual(controller.popupDepth, 1)
        browser.webViewDidClose(first)
        XCTAssertEqual(controller.popupDepth, 1, "an already-closed popup is ignored")
        browser.webViewDidClose(browser.mainWebView)
        XCTAssertEqual(controller.popupDepth, 1, "the main page never closes")
        controller.closePopup()
        XCTAssertFalse(browser.subviews.contains(second))
        XCTAssertEqual(controller.popupDepth, 0)
    }

    func testNavigationWithoutHistoryOrPopupsDoesNothing() {
        let (browser, controller) = makeBrowser()
        controller.closePopup()
        controller.goBack()
        controller.goForward()
        XCTAssertEqual(controller.popupDepth, 0)
        XCTAssertTrue(browser.subviews.contains(browser.mainWebView))
    }

    /// Reload starts a load of the current page; pressed while it loads, it
    /// stops, and the page settles.
    func testReloadStartsAndStopsTheLoad() async {
        let (browser, controller) = makeBrowser()
        browser.mainWebView.stopLoading()
        controller.reload()
        XCTAssertTrue(browser.mainWebView.isLoading, "reload starts a load")
        controller.reload()
        await waitUntil { !browser.mainWebView.isLoading }
    }

    /// Navigation events publish the browser's live state to the chrome.
    func testNavigationEventsPublishTheChrome() throws {
        let (browser, controller) = makeBrowser()
        let popup = try XCTUnwrap(Self.openPopup(in: browser))
        controller.apply(canGoBack: true, canGoForward: true, isLoading: true, currentURL: "stale", popupDepth: 5)
        browser.webView(popup, didCommit: nil)
        XCTAssertEqual(controller.popupDepth, 1)
        XCTAssertEqual(controller.currentURL, "")

        controller.apply(canGoBack: true, canGoForward: true, isLoading: true, currentURL: "stale", popupDepth: 5)
        browser.webView(popup, didFail: nil, withError: URLError(.cancelled))
        XCTAssertEqual(controller.popupDepth, 1)

        controller.apply(canGoBack: true, canGoForward: true, isLoading: true, currentURL: "stale", popupDepth: 5)
        browser.webView(popup, didFailProvisionalNavigation: nil, withError: URLError(.cannotFindHost))
        XCTAssertEqual(controller.popupDepth, 1)

        controller.apply(canGoBack: true, canGoForward: true, isLoading: true, currentURL: "stale", popupDepth: 5)
        browser.webView(popup, didFinish: nil)
        XCTAssertEqual(controller.popupDepth, 1)
        XCTAssertFalse(controller.canGoForward)
    }

    // MARK: - Return detection

    func testReturnFiresOnceAfterTheAuthHost() {
        let (browser, _) = makeBrowser()
        browser.handleFinished(Self.returnURL, isPopup: false)
        XCTAssertTrue(returned.isEmpty, "the first load of the provider page is not a sign-in")

        browser.handleFinished(Self.authURL, isPopup: false)
        XCTAssertEqual(authSeen, 1)
        browser.handleFinished(Self.returnURL, isPopup: false)
        browser.handleFinished(Self.returnURL, isPopup: false)
        XCTAssertEqual(returned, [Self.returnURL], "fires once per sign-in")

        browser.rearmReturn()
        browser.handleFinished(Self.returnURL, isPopup: false)
        XCTAssertEqual(returned, [Self.returnURL, Self.returnURL], "re-armed after an empty capture")
    }

    func testReturnInsideAPopupWaitsForThePopupToClose() throws {
        let (browser, _) = makeBrowser()
        let popup = try XCTUnwrap(Self.openPopup(in: browser))
        browser.handleFinished(Self.authURL, isPopup: true)
        browser.handleFinished(Self.returnURL, isPopup: true)
        XCTAssertTrue(returned.isEmpty)

        // The main page returned while the popup is still up.
        browser.handleFinished(Self.returnURL, isPopup: false)
        XCTAssertTrue(returned.isEmpty)
        _ = popup
    }

    /// A popup opened and cancelled before the main page returned does not
    /// capture when it closes.
    func testCancelledPopupDoesNotCapture() throws {
        let (browser, _) = makeBrowser(blankIsReturnPage: true)
        let popup = try XCTUnwrap(Self.openPopup(in: browser))
        browser.handleFinished(Self.authURL, isPopup: true)
        browser.webViewDidClose(popup)
        XCTAssertTrue(returned.isEmpty)
    }

    /// Once the main page is back on the return page, closing the last popup
    /// fires the return that was held back.
    func testClosingThePopupFiresTheHeldReturn() async throws {
        let (browser, _) = makeBrowser(blankIsReturnPage: true)
        await waitUntil { browser.mainWebView.url != nil }
        let main = try XCTUnwrap(browser.mainWebView.url)

        let popup = try XCTUnwrap(Self.openPopup(in: browser))
        browser.handleFinished(Self.authURL, isPopup: true)
        browser.handleFinished(main, isPopup: false)
        XCTAssertTrue(returned.isEmpty, "held while the popup is open")

        browser.webViewDidClose(popup)
        XCTAssertEqual(returned, [main])
    }

    /// Replacing the predicates re-targets the return gate.
    func testUpdatedPredicatesReachTheGate() {
        let (browser, _) = makeBrowser()
        browser.isAuthHost = { host, _ in host == "sso.example.org" }
        browser.isReturnPage = { $0.host == "app.example.org" }
        browser.handleFinished(Self.authURL, isPopup: false)
        XCTAssertEqual(authSeen, 0)
        browser.handleFinished(URL(string: "https://sso.example.org/")!, isPopup: false)
        XCTAssertEqual(authSeen, 1)
        browser.handleFinished(URL(string: "https://app.example.org/")!, isPopup: false)
        XCTAssertEqual(returned.map(\.host), ["app.example.org"])
    }

    func testControllerWithoutABrowserIgnoresCommands() {
        let controller = SignInBrowserController()
        controller.goBack()
        controller.goForward()
        controller.reload()
        controller.closePopup()
        controller.rearmReturn()
        XCTAssertFalse(controller.hasPopup)
    }

    func testReturnGateEventsHaveNames() {
        for event in SignInReturnGate.Event.allCases {
            XCTAssertEqual(SignInReturnGate.Event(rawValue: event.rawValue), event)
            XCTAssertFalse(event.displayName.isEmpty)
        }
        XCTAssertEqual(SignInReturnGate.Event.returnPage.displayName, "Return page")
        XCTAssertNil(SignInReturnGate.Event(rawValue: "elsewhere"))
    }

    private func waitUntil(timeout: TimeInterval = 10, _ condition: () -> Bool) async {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition(), Date() < deadline {
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertTrue(condition(), "timed out")
    }
}
