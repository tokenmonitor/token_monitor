import SwiftUI
@testable import TokenMon
import XCTest

/// Serves `json` for the release feed and `zip` for the release download.
private final class FeedStubURLProtocol: URLProtocol {
    static var json = ""
    static var zip = Data()

    override static func canInit(with _: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        let isZip = request.url?.pathExtension == "zip"
        let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: isZip ? Self.zip : Data(Self.json.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}

/// The menu panel's "Update available" pill appears only with a pending
/// release, names the version, and installs it when pressed.
@MainActor
final class UpdateAvailableIndicatorTests: XCTestCase {
    private var suiteName: String!
    private var defaults: UserDefaults!
    private var session: URLSession!
    private var folder: URL!
    private var openedURLs: [URL] = []

    override func setUp() async throws {
        suiteName = "UpdateAvailableIndicator-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [FeedStubURLProtocol.self]
        session = URLSession(configuration: configuration)
        folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("UpdateAvailableIndicator-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        FeedStubURLProtocol.json = Self.release(digest: nil)
        FeedStubURLProtocol.zip = Data()
        openedURLs = []
    }

    override func tearDown() async throws {
        defaults.removePersistentDomain(forName: suiteName)
        try? FileManager.default.removeItem(at: folder)
    }

    private static func release(digest: String?) -> String {
        let digestField = digest.map { #","digest":"sha256:\#($0)""# } ?? ""
        return """
        {"tag_name":"v1.6.0","draft":false,"prerelease":false,
         "html_url":"https://github.com/tokenmonitor/token_monitor/releases/tag/v1.6.0",
         "assets":[{"name":"TokenMon-1.6.0.zip",
           "browser_download_url":"https://github.com/tokenmonitor/token_monitor/releases/download/v1.6.0/TokenMon-1.6.0.zip"\(digestField)}]}
        """
    }

    private func makeChecker(
        installedAppURL: URL = URL(fileURLWithPath: "/nonexistent/TokenMon.app"),
        relaunch: @escaping @MainActor (URL) throws -> Void = { _ in }
    ) -> UpdateChecker {
        UpdateChecker(
            settings: AppSettings(defaults: defaults),
            currentVersion: AppVersion("1.5.0"),
            session: session,
            bundleIdentifier: AppBundleFixture.bundleID,
            installedAppURL: installedAppURL,
            openURL: { [weak self] in self?.openedURLs.append($0) },
            downloadSession: session,
            relaunch: relaunch
        )
    }

    func testIndicatorIsEmptyWithoutAChecker() {
        let size = ViewHarness.render(UpdateAvailableIndicator())
        XCTAssertEqual(size.height, 0, accuracy: 0.5)
        XCTAssertTrue(ViewTree(UpdateAvailableIndicator()).texts.isEmpty)
    }

    func testIndicatorIsEmptyUntilAReleaseIsFound() async {
        let checker = makeChecker()
        let empty = ViewHarness.render(UpdateAvailableIndicator().environment(\.updateChecker, checker))
        XCTAssertEqual(empty.height, 0, accuracy: 0.5)
        XCTAssertTrue(ViewTree(UpdateAvailablePill(checker: checker)).texts.isEmpty)

        await checker.checkNow()
        let shown = ViewHarness.renderBothAppearances(UpdateAvailableIndicator().environment(\.updateChecker, checker))
        XCTAssertGreaterThan(shown.height, 10)
    }

    func testPillNamesTheVersionAndShowsTheStatus() async {
        let checker = makeChecker()
        await checker.checkNow()
        var tree = ViewTree(UpdateAvailablePill(checker: checker))
        XCTAssertTrue(tree.shows("Update available"))
        XCTAssertTrue(tree.shows("Download, verify, and install TokenMon %@"))
        XCTAssertTrue(tree.texts.contains("1.6.0"))
        XCTAssertFalse(tree.shows("cannot replace itself"))

        // This copy cannot replace itself, so the pill opens the release page and says so.
        tree.press("Update available")
        await waitUntil { !self.openedURLs.isEmpty }
        XCTAssertEqual(openedURLs.map(\.lastPathComponent), ["v1.6.0"])
        tree = ViewTree(UpdateAvailablePill(checker: checker))
        XCTAssertTrue(tree.shows("TokenMon cannot replace itself from this folder."))
        ViewHarness.render(UpdateAvailablePill(checker: checker))
    }

    func testPillSaysInstallingWhileTheUpdateInstalls() async throws {
        let installed = try AppBundleFixture.makeApp(in: folder, version: "1.5.0", signed: false)
        let fixture = folder.appendingPathComponent("fixture", isDirectory: true)
        try FileManager.default.createDirectory(at: fixture, withIntermediateDirectories: true)
        let app = try AppBundleFixture.makeApp(in: fixture, version: "1.6.0")
        FeedStubURLProtocol.zip = try AppBundleFixture.zip(app, to: fixture.appendingPathComponent("release.zip"))
        FeedStubURLProtocol.json = Self.release(digest: AppBundleFixture.sha256(FeedStubURLProtocol.zip))

        var duringInstall: ViewTree?
        var checker: UpdateChecker?
        checker = makeChecker(installedAppURL: installed) { _ in
            if let checker { duringInstall = ViewTree(UpdateAvailablePill(checker: checker)) }
        }
        await checker?.checkNow()
        await checker?.installAvailableUpdate()

        XCTAssertEqual(duringInstall?.shows("Installing update…"), true)
        XCTAssertEqual(duringInstall?.shows("Installing TokenMon %@"), true)
        XCTAssertEqual(try AppBundleFixture.bundleVersion(of: installed), "1.6.0")
    }

    private func waitUntil(timeout: TimeInterval = 5, _ condition: () -> Bool) async {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition(), Date() < deadline {
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTAssertTrue(condition(), "timed out")
    }
}
