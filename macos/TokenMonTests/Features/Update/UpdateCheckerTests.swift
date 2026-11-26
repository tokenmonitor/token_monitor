@testable import TokenMon
import XCTest

/// Serves a canned response (or throws) for every request.
private class StubURLProtocol: URLProtocol {
    static var handler: ((URLRequest) throws -> (URLResponse, Data))?
    static var requestCount = 0

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        Self.requestCount += 1
        guard let handler = Self.handler else {
            client?.urlProtocol(self, didFailWithError: URLError(.badServerResponse))
            return
        }
        do {
            let (response, data) = try handler(request)
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {}
}

@MainActor
final class UpdateCheckerTests: XCTestCase {
    private var suiteName: String!
    private var defaults: UserDefaults!
    private var session: URLSession!

    override func setUp() async throws {
        suiteName = "UpdateCheckerTests-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [StubURLProtocol.self]
        session = URLSession(configuration: config)
        StubURLProtocol.handler = nil
        StubURLProtocol.requestCount = 0
        openedURLs = []
        relaunched = []
    }

    override func tearDown() async throws {
        StubURLProtocol.handler = nil
        defaults.removePersistentDomain(forName: suiteName)
    }

    private var openedURLs: [URL] = []
    private var relaunched: [URL] = []

    private func makeChecker(
        settings: AppSettings? = nil,
        currentVersion: AppVersion? = AppVersion("1.5.0"),
        bundleIdentifier: String? = "com.modelmonitor.app",
        installedAppURL: URL = URL(fileURLWithPath: "/nonexistent/TokenMon.app"),
        relaunch: (@MainActor (URL) throws -> Void)? = nil
    ) -> UpdateChecker {
        UpdateChecker(
            settings: settings ?? AppSettings(defaults: defaults),
            currentVersion: currentVersion,
            session: session,
            bundleIdentifier: bundleIdentifier,
            installedAppURL: installedAppURL,
            openURL: { [weak self] in self?.openedURLs.append($0) },
            downloadSession: session,
            relaunch: relaunch ?? { [weak self] in self?.relaunched.append($0) }
        )
    }

    /// An `.app` folder inside a temporary folder; read-only when `writable` is false.
    private func makeInstalledApp(writable: Bool) throws -> URL {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("UpdateCheckerTests-\(UUID().uuidString)", isDirectory: true)
        let app = folder.appendingPathComponent("TokenMon.app", isDirectory: true)
        try FileManager.default.createDirectory(at: app, withIntermediateDirectories: true)
        if !writable {
            try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: folder.path)
        }
        addTeardownBlock {
            try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: folder.path)
            try? FileManager.default.removeItem(at: folder)
        }
        return app
    }

    private static let releaseWithAssets = """
    {"tag_name":"v1.6.0","draft":false,"prerelease":false,
     "html_url":"https://github.com/tokenmonitor/token_monitor/releases/tag/v1.6.0",
     "assets":[
       {"name":"TokenMon-1.6.0.zip",
        "browser_download_url":"https://github.com/tokenmonitor/token_monitor/releases/download/v1.6.0/TokenMon-1.6.0.zip"},
       {"name":"TokenMon-1.6.0.pkg",
        "browser_download_url":"https://github.com/tokenmonitor/token_monitor/releases/download/v1.6.0/TokenMon-1.6.0.pkg"}
     ]}
    """

    private func respond(status: Int, body: String) {
        StubURLProtocol.handler = { request in
            let response = HTTPURLResponse(
                url: request.url!,
                statusCode: status,
                httpVersion: nil,
                headerFields: ["Content-Type": "application/json"]
            )!
            return (response, Data(body.utf8))
        }
    }

    func testNewerReleaseIsPublished() async {
        respond(status: 200, body: """
        {"tag_name":"v1.5.1","draft":false,"prerelease":false,
         "html_url":"https://github.com/tokenmonitor/token_monitor/releases/tag/v1.5.1"}
        """)
        let checker = makeChecker()
        await checker.checkNow()
        XCTAssertEqual(checker.availableRelease?.version.description, "1.5.1")
        XCTAssertNil(checker.lastError)
    }

    func testSameOrOlderReleaseIsIgnored() async {
        respond(status: 200, body: #"{"tag_name":"v1.5.0","draft":false,"prerelease":false}"#)
        let checker = makeChecker()
        await checker.checkNow()
        XCTAssertNil(checker.availableRelease)
    }

    func testDraftAndPrereleaseAreIgnored() async {
        respond(status: 200, body: #"{"tag_name":"v9.9.9","draft":true,"prerelease":false}"#)
        let checker = makeChecker()
        await checker.checkNow()
        XCTAssertNil(checker.availableRelease)

        respond(status: 200, body: #"{"tag_name":"v9.9.9","draft":false,"prerelease":true}"#)
        await checker.checkNow()
        XCTAssertNil(checker.availableRelease)
    }

    func testRateLimit403ReportsFriendlyError() async {
        respond(status: 403, body: "rate limited")
        let checker = makeChecker()
        await checker.checkNow()
        XCTAssertEqual(checker.lastError, "Update check failed: GitHub rate limit reached; will retry later")
        XCTAssertNil(checker.availableRelease)
    }

    func testServerErrorIsReported() async {
        respond(status: 500, body: "boom")
        let checker = makeChecker()
        await checker.checkNow()
        XCTAssertEqual(checker.lastError, "Update check failed: HTTP 500")
    }

    func testMissingBundleVersionSkipsCheck() async {
        respond(status: 200, body: #"{"tag_name":"v9.9.9"}"#)
        let checker = makeChecker(currentVersion: nil)
        await checker.checkNow()
        XCTAssertNil(checker.availableRelease)
        XCTAssertNil(checker.lastError)
        XCTAssertEqual(StubURLProtocol.requestCount, 0)
    }

    func testManualCheckRunsWhenAutomaticChecksAreOff() async {
        defaults.set(false, forKey: "checksForUpdates")
        respond(status: 200, body: #"{"tag_name":"v1.5.0","draft":false,"prerelease":false}"#)
        let checker = makeChecker()
        await checker.checkManually()
        XCTAssertEqual(StubURLProtocol.requestCount, 1)
        XCTAssertEqual(checker.statusMessage, "You're up to date.")
    }

    func testManualCheckSurfacesANewerRelease() async {
        respond(status: 200, body: """
        {"tag_name":"v1.5.1","draft":false,"prerelease":false,
         "html_url":"https://github.com/tokenmonitor/token_monitor/releases/tag/v1.5.1",
         "assets":[{"name":"TokenMon-1.5.1.zip",
           "browser_download_url":"https://github.com/tokenmonitor/token_monitor/releases/download/v1.5.1/TokenMon-1.5.1.zip"}]}
        """)
        let checker = makeChecker()
        await checker.checkManually()
        XCTAssertEqual(checker.actionTitle, "Update to 1.5.1…")
        XCTAssertEqual(
            checker.availableRelease?.archiveURL?.lastPathComponent,
            "TokenMon-1.5.1.zip"
        )
        XCTAssertNil(checker.statusMessage)
    }

    func testChecksDisabledSkipsRequest() async {
        defaults.set(false, forKey: "checksForUpdates")
        respond(status: 200, body: #"{"tag_name":"v9.9.9"}"#)
        let checker = makeChecker()
        await checker.checkNow()
        XCTAssertEqual(StubURLProtocol.requestCount, 0)
    }

    /// A copy that cannot replace itself is offered the installer package.
    func testNotWritableInstallOffersInstallerPackage() async throws {
        respond(status: 200, body: Self.releaseWithAssets)
        let checker = try makeChecker(installedAppURL: makeInstalledApp(writable: false))
        await checker.checkNow()
        await checker.installAvailableUpdate()
        XCTAssertEqual(openedURLs.map(\.lastPathComponent), ["TokenMon-1.6.0.pkg"])
        XCTAssertEqual(StubURLProtocol.requestCount, 1, "nothing is downloaded in-app")
        XCTAssertFalse(checker.isInstalling)
    }

    /// Without the asset digest the zip cannot be verified, so it is not installed.
    func testMissingDigestOpensReleasePageInsteadOfInstalling() async throws {
        respond(status: 200, body: Self.releaseWithAssets)
        let checker = try makeChecker(installedAppURL: makeInstalledApp(writable: true))
        await checker.checkNow()
        await checker.installAvailableUpdate()
        XCTAssertEqual(openedURLs.map(\.lastPathComponent), ["v1.6.0"])
        XCTAssertEqual(checker.statusMessage, "The release has no checksum to verify — opening the release page.")
        XCTAssertEqual(StubURLProtocol.requestCount, 1, "the zip is never requested")
    }

    // MARK: - Installing

    private static let zipURL = "https://github.com/tokenmonitor/token_monitor/releases/download/v1.6.0/TokenMon-1.6.0.zip"

    /// A v1.6.0 release whose zip asset carries `digest` (`sha256:<hex>`).
    private static func release(zipDigest digest: String?, package: Bool = false) -> String {
        var assets = [#"{"name":"TokenMon-1.6.0.zip","browser_download_url":"\#(zipURL)""#
            + (digest.map { #","digest":"sha256:\#($0)""# } ?? "") + "}"]
        if package {
            assets.append(#"{"name":"TokenMon-1.6.0.pkg","browser_download_url":"https://github.com/tokenmonitor/token_monitor/releases/download/v1.6.0/TokenMon-1.6.0.pkg"}"#)
        }
        return """
        {"tag_name":"v1.6.0","draft":false,"prerelease":false,
         "html_url":"https://github.com/tokenmonitor/token_monitor/releases/tag/v1.6.0",
         "assets":[\(assets.joined(separator: ","))]}
        """
    }

    /// Serves the release JSON for the feed and `zip` for the download.
    private func serve(release json: String, zip: Data = Data()) {
        StubURLProtocol.handler = { request in
            let isZip = request.url?.absoluteString == Self.zipURL
            let response = HTTPURLResponse(url: request.url!, statusCode: 200, httpVersion: nil, headerFields: nil)!
            return (response, isZip ? zip : Data(json.utf8))
        }
    }

    /// An installed v1.5.0 app plus a served, signed v1.6.0 release zip.
    private func stageRelease() throws -> (installed: URL, zip: Data) {
        let installed = try makeInstalledApp(writable: true)
        try FileManager.default.removeItem(at: installed)
        _ = try AppBundleFixture.makeApp(in: installed.deletingLastPathComponent(), version: "1.5.0", signed: false)
        let fixture = installed.deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("fixture-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: fixture, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: fixture) }
        let app = try AppBundleFixture.makeApp(in: fixture, version: "1.6.0")
        let zip = try AppBundleFixture.zip(app, to: fixture.appendingPathComponent("release.zip"))
        return (installed, zip)
    }

    func testInstallsTheVerifiedUpdateThenRelaunches() async throws {
        let (installed, zip) = try stageRelease()
        serve(release: Self.release(zipDigest: AppBundleFixture.sha256(zip)), zip: zip)
        var titleWhileInstalling = ""
        var couldActWhileInstalling = true
        var checker: UpdateChecker?
        checker = makeChecker(installedAppURL: installed) { [weak self] app in
            titleWhileInstalling = checker?.actionTitle ?? ""
            couldActWhileInstalling = checker?.canAct ?? true
            self?.relaunched.append(app)
        }
        await checker?.checkNow()
        XCTAssertEqual(checker?.actionTitle, "Update to 1.6.0…")
        await checker?.performPrimaryAction()

        XCTAssertEqual(relaunched.map(\.standardizedFileURL.path), [installed.standardizedFileURL.path])
        XCTAssertEqual(try AppBundleFixture.bundleVersion(of: installed), "1.6.0")
        XCTAssertEqual(titleWhileInstalling, "Installing update…")
        XCTAssertFalse(couldActWhileInstalling)
        XCTAssertEqual(checker?.isInstalling, false)
        XCTAssertTrue(openedURLs.isEmpty)
    }

    func testFailedRelaunchAsksTheUserToReopen() async throws {
        let (installed, zip) = try stageRelease()
        serve(release: Self.release(zipDigest: AppBundleFixture.sha256(zip)), zip: zip)
        let checker = makeChecker(installedAppURL: installed) { _ in throw CocoaError(.executableLoad) }
        await checker.checkNow()
        await checker.installAvailableUpdate()
        XCTAssertEqual(checker.statusMessage, "TokenMon 1.6.0 is installed. Quit and reopen TokenMon to finish.")
        XCTAssertEqual(try AppBundleFixture.bundleVersion(of: installed), "1.6.0")
    }

    func testFailedInstallOpensTheReleasePage() async throws {
        let (installed, zip) = try stageRelease()
        serve(release: Self.release(zipDigest: String(repeating: "0", count: 64)), zip: zip)
        let checker = makeChecker(installedAppURL: installed)
        await checker.checkNow()
        await checker.installAvailableUpdate()
        XCTAssertEqual(checker.statusMessage, "The update failed verification: The download does not match the release checksum.")
        XCTAssertEqual(openedURLs.map(\.lastPathComponent), ["v1.6.0"])
        XCTAssertTrue(relaunched.isEmpty)
        XCTAssertEqual(try AppBundleFixture.bundleVersion(of: installed), "1.5.0")
    }

    func testReleaseWithoutAZipOpensTheReleasePage() async {
        respond(status: 200, body: """
        {"tag_name":"v1.6.0","draft":false,"prerelease":false,
         "html_url":"https://github.com/tokenmonitor/token_monitor/releases/tag/v1.6.0"}
        """)
        let checker = makeChecker()
        await checker.checkNow()
        await checker.installAvailableUpdate()
        XCTAssertEqual(checker.statusMessage, "No installer was published — opening the release page.")
        XCTAssertEqual(openedURLs.map(\.lastPathComponent), ["v1.6.0"])
    }

    func testBuildWithoutBundleIdentifierOpensTheReleasePage() async throws {
        serve(release: Self.release(zipDigest: String(repeating: "a", count: 64)))
        let checker = try makeChecker(bundleIdentifier: nil, installedAppURL: makeInstalledApp(writable: true))
        await checker.checkNow()
        await checker.installAvailableUpdate()
        XCTAssertEqual(
            checker.statusMessage,
            "This build has no version or bundle identifier to check against — opening the release page."
        )
        XCTAssertEqual(StubURLProtocol.requestCount, 1, "the zip is never requested")
    }

    func testTranslocatedCopyIsAskedToMoveIntoApplications() async {
        serve(release: Self.release(zipDigest: String(repeating: "a", count: 64), package: true))
        let translocated = URL(fileURLWithPath: "/private/var/folders/xy/AppTranslocation/ABCD/d/TokenMon.app")
        let checker = makeChecker(installedAppURL: translocated)
        await checker.checkNow()
        await checker.installAvailableUpdate()
        XCTAssertEqual(checker.statusMessage, "Move TokenMon into Applications, then update.")
        XCTAssertEqual(openedURLs.map(\.lastPathComponent), ["v1.6.0"])
    }

    func testReadOnlyCopyWithoutAPackageOpensTheReleasePage() async throws {
        serve(release: Self.release(zipDigest: String(repeating: "a", count: 64)))
        let checker = try makeChecker(installedAppURL: makeInstalledApp(writable: false))
        await checker.checkNow()
        await checker.installAvailableUpdate()
        XCTAssertEqual(checker.statusMessage, "TokenMon cannot replace itself from this folder.")
        XCTAssertEqual(openedURLs.map(\.lastPathComponent), ["v1.6.0"])
    }

    func testInstallWithNothingPendingDoesNothing() async {
        let checker = makeChecker()
        await checker.installAvailableUpdate()
        XCTAssertTrue(openedURLs.isEmpty)
        XCTAssertNil(checker.statusMessage)
    }

    func testPrimaryActionChecksWhenNothingIsPending() async {
        respond(status: 200, body: #"{"tag_name":"v1.5.0","draft":false,"prerelease":false}"#)
        let checker = makeChecker()
        XCTAssertEqual(checker.actionTitle, "Check for Updates")
        XCTAssertTrue(checker.canAct)
        await checker.performPrimaryAction()
        XCTAssertEqual(checker.statusMessage, "You're up to date.")
        XCTAssertTrue(openedURLs.isEmpty)
    }

    // MARK: - Checking

    func testManualCheckWithoutAVersionExplainsWhy() async {
        let checker = makeChecker(currentVersion: nil)
        await checker.checkManually()
        XCTAssertEqual(checker.statusMessage, "This build has no version number to compare.")
        XCTAssertEqual(StubURLProtocol.requestCount, 0)
    }

    func testNetworkFailureIsReported() async {
        StubURLProtocol.handler = { _ in throw URLError(.notConnectedToInternet) }
        let checker = makeChecker()
        await checker.checkManually()
        XCTAssertEqual(checker.lastError?.hasPrefix("Update check failed: "), true)
        XCTAssertEqual(checker.statusMessage, checker.lastError)
    }

    func testNonHTTPResponseIsReported() async {
        StubURLProtocol.handler = { request in
            (URLResponse(url: request.url!, mimeType: nil, expectedContentLength: 0, textEncodingName: nil), Data())
        }
        let checker = makeChecker()
        await checker.checkNow()
        XCTAssertEqual(checker.lastError, "Update check failed: No HTTP response")
    }

    func testStartPollsTheFeedAndSettingOffClearsTheBanner() async throws {
        respond(status: 200, body: Self.releaseWithAssets)
        let settings = AppSettings(defaults: defaults)
        let checker = makeChecker(settings: settings)
        checker.start()
        defer { checker.stop() }
        try await waitUntil { checker.availableRelease != nil }
        XCTAssertEqual(checker.availableRelease?.version.description, "1.6.0")

        settings.checksForUpdates = false
        checker.settingChanged()
        XCTAssertNil(checker.availableRelease)
        XCTAssertNil(checker.statusMessage)
        XCTAssertNil(checker.lastError)
    }

    func testStartDoesNothingWhileChecksAreOff() async throws {
        defaults.set(false, forKey: "checksForUpdates")
        respond(status: 200, body: Self.releaseWithAssets)
        let checker = makeChecker()
        checker.start()
        try await Task.sleep(nanoseconds: 100_000_000)
        XCTAssertEqual(StubURLProtocol.requestCount, 0)
        XCTAssertNil(checker.availableRelease)
    }

    func testTurningChecksBackOnStartsPolling() async throws {
        defaults.set(false, forKey: "checksForUpdates")
        respond(status: 200, body: Self.releaseWithAssets)
        let settings = AppSettings(defaults: defaults)
        let checker = makeChecker(settings: settings)
        settings.checksForUpdates = true
        checker.settingChanged()
        defer { checker.stop() }
        try await waitUntil { checker.availableRelease != nil }
        XCTAssertEqual(StubURLProtocol.requestCount, 1)
    }

    func testDownloadSessionRefusesRedirectsOffGitHub() {
        XCTAssertTrue(UpdateChecker.makeDownloadSession().delegate is TrustedReleaseRedirect)
    }

    private func waitUntil(timeout: TimeInterval = 5, _ condition: () -> Bool) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition() {
            guard Date() < deadline else { return XCTFail("timed out") }
            try await Task.sleep(nanoseconds: 10_000_000)
        }
    }
}
