@testable import TokenMon
import XCTest

/// Serves `body` for every request and counts requests. A test can change the
/// status, the reported URL and length, or fail the request or the stream.
private final class ArchiveStubURLProtocol: URLProtocol {
    static var body = Data()
    static var requestCount = 0
    static var status = 200
    static var responseURL: URL?
    static var declaredLength: Int?
    static var failure: URLError?
    static var failsAfterBody = false

    static func reset() {
        body = Data()
        requestCount = 0
        status = 200
        responseURL = nil
        declaredLength = nil
        failure = nil
        failsAfterBody = false
    }

    override static func canInit(with _: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        Self.requestCount += 1
        if let failure = Self.failure {
            client?.urlProtocol(self, didFailWithError: failure)
            return
        }
        let response = HTTPURLResponse(
            url: Self.responseURL ?? request.url!,
            statusCode: Self.status,
            httpVersion: nil,
            headerFields: ["Content-Length": String(Self.declaredLength ?? Self.body.count)]
        )!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Self.body)
        if Self.failsAfterBody {
            client?.urlProtocol(self, didFailWithError: URLError(.networkConnectionLost))
        } else {
            client?.urlProtocolDidFinishLoading(self)
        }
    }

    override func stopLoading() {}
}

/// Exercises the download → verify → swap path against ad-hoc signed fixture
/// apps in a temporary folder. Nothing touches /Applications or the network.
final class AppInstallerTests: XCTestCase {
    private static let bundleID = "com.modelmonitor.app"
    private static let releaseURL = URL(
        string: "https://github.com/tokenmonitor/token_monitor/releases/download/v2.0.0/TokenMon-2.0.0.zip"
    )!

    private var root: URL!
    private var destination: URL!
    private var session: URLSession!

    override func setUpWithError() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("AppInstallerTests-\(UUID().uuidString)", isDirectory: true)
        let applications = root.appendingPathComponent("Applications", isDirectory: true)
        try FileManager.default.createDirectory(at: applications, withIntermediateDirectories: true)
        destination = try makeApp(in: applications, version: "1.5.0", signed: false)

        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ArchiveStubURLProtocol.self]
        session = URLSession(configuration: configuration)
        ArchiveStubURLProtocol.reset()
    }

    override func tearDownWithError() throws {
        if let root {
            _ = try? AppBundleFixture.run("/bin/chmod", ["-R", "u+w", root.path])
            try? FileManager.default.removeItem(at: root)
        }
    }

    // MARK: - Install path

    func testInstallsVerifiedUpdateInPlace() async throws {
        let sha = try serve(makeApp(in: fixtureFolder(), version: "2.0.0"))
        let installed = try await install(sha256: sha)

        XCTAssertEqual(installed.standardizedFileURL.path, destination.standardizedFileURL.path)
        XCTAssertEqual(try bundleVersion(of: destination), "2.0.0")
        let siblings = try FileManager.default.contentsOfDirectory(atPath: destination.deletingLastPathComponent().path)
        XCTAssertEqual(siblings, ["TokenMon.app"], "the staged copy is swapped in, not left beside it")
    }

    func testChecksumMismatchIsRefused() async throws {
        try serve(makeApp(in: fixtureFolder(), version: "2.0.0"))
        await assertInstallFails(sha256: String(repeating: "0", count: 64)) {
            guard case .verification = $0 else { return XCTFail("expected verification failure, got \($0)") }
        }
    }

    func testWrongBundleIdentifierIsRefused() async throws {
        let sha = try serve(makeApp(in: fixtureFolder(), version: "2.0.0", bundleID: "com.example.other"))
        await assertInstallFails(sha256: sha) {
            guard case .verification = $0 else { return XCTFail("expected verification failure, got \($0)") }
        }
    }

    func testDowngradeIsRefused() async throws {
        let sha = try serve(makeApp(in: fixtureFolder(), version: "1.4.0"))
        await assertInstallFails(sha256: sha) {
            XCTAssertEqual($0, .verification("The download is not newer than this version."))
        }
    }

    func testSameVersionIsRefused() async throws {
        let sha = try serve(makeApp(in: fixtureFolder(), version: "1.5.0"))
        await assertInstallFails(sha256: sha) {
            XCTAssertEqual($0, .verification("The download is not newer than this version."))
        }
    }

    func testUnsignedAppIsRefused() async throws {
        let sha = try serve(makeApp(in: fixtureFolder(), version: "2.0.0", signed: false))
        await assertInstallFails(sha256: sha) {
            guard case .verification = $0 else { return XCTFail("expected verification failure, got \($0)") }
        }
    }

    func testAppModifiedAfterSigningIsRefused() async throws {
        let app = try makeApp(in: fixtureFolder(), version: "2.0.0")
        try Data("tampered".utf8).write(to: app.appendingPathComponent("Contents/Resources/data.txt"))
        let sha = try serve(app)
        await assertInstallFails(sha256: sha) {
            XCTAssertEqual($0, .verification("The code signature is not valid for TokenMon."))
        }
    }

    func testSymbolicLinkInArchiveIsRefused() async throws {
        let app = try makeApp(in: fixtureFolder(), version: "2.0.0")
        try FileManager.default.createSymbolicLink(
            at: app.appendingPathComponent("Contents/Resources/link"),
            withDestinationURL: URL(fileURLWithPath: "/etc/hosts")
        )
        let sha = try serve(app)
        await assertInstallFails(sha256: sha) {
            XCTAssertEqual($0, .archive("The release zip contains a symbolic link."))
        }
    }

    func testAppNestedBelowTopLevelIsRefused() async throws {
        let folder = fixtureFolder()
        let nested = folder.appendingPathComponent("nested", isDirectory: true)
        try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
        _ = try makeApp(in: nested, version: "2.0.0")
        let sha = try serve(folder)
        await assertInstallFails(sha256: sha) {
            XCTAssertEqual($0, .archive("The release zip does not contain TokenMon.app at its top level."))
        }
    }

    func testDownloadOutsideThisRepositoryIsRefused() async throws {
        let sha = try serve(makeApp(in: fixtureFolder(), version: "2.0.0"))
        let elsewhere = URL(string: "https://github.com/someone/else/releases/download/v2.0.0/TokenMon-2.0.0.zip")!
        await assertInstallFails(sha256: sha, from: elsewhere) {
            guard case .download = $0 else { return XCTFail("expected download failure, got \($0)") }
        }
        XCTAssertEqual(ArchiveStubURLProtocol.requestCount, 0)
    }

    func testReadOnlyInstallFolderIsRefused() async throws {
        let sha = try serve(makeApp(in: fixtureFolder(), version: "2.0.0"))
        try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: destination.deletingLastPathComponent().path)
        await assertInstallFails(sha256: sha) { XCTAssertEqual($0, .notWritable) }
        XCTAssertEqual(ArchiveStubURLProtocol.requestCount, 0)
    }

    // MARK: - Pieces

    func testDownloadStopsAtSizeLimit() async throws {
        ArchiveStubURLProtocol.body = Data(repeating: 7, count: 200 * 1024)
        let file = root.appendingPathComponent("big.zip")
        do {
            _ = try await AppInstaller.download(Self.releaseURL, to: file, session: session, limit: 100 * 1024)
            XCTFail("expected the size limit to stop the download")
        } catch let failure as AppInstaller.Failure {
            XCTAssertEqual(failure, .download("The release file is unexpectedly large."))
        }
    }

    func testDownloadReturnsSHA256OfBody() async throws {
        ArchiveStubURLProtocol.body = Data((0..<100_000).map { UInt8($0 % 251) })
        let file = root.appendingPathComponent("body.zip")
        let digest = try await AppInstaller.download(Self.releaseURL, to: file, session: session)
        XCTAssertEqual(digest, Self.sha256(ArchiveStubURLProtocol.body))
        XCTAssertEqual(try Data(contentsOf: file), ArchiveStubURLProtocol.body)
    }

    func testCanReplaceRunningAppChecksTheGivenBundle() throws {
        XCTAssertTrue(AppInstaller.canReplaceRunningApp(bundleURL: destination))
        let translocated = URL(fileURLWithPath: "/private/var/folders/xy/AppTranslocation/ABCD/d/TokenMon.app")
        XCTAssertFalse(AppInstaller.canReplaceRunningApp(bundleURL: translocated))
        XCTAssertFalse(AppInstaller.canReplaceRunningApp(bundleURL: root.appendingPathComponent("TokenMon")))

        try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: destination.path)
        XCTAssertFalse(AppInstaller.canReplaceRunningApp(bundleURL: destination))
    }

    func testDeclaredUncompressedBytesParsesZipinfoSummary() {
        XCTAssertEqual(
            AppInstaller.declaredUncompressedBytes(in: "12 files, 3456 bytes uncompressed, 1200 bytes compressed:  65.3%\n"),
            3456
        )
        XCTAssertNil(AppInstaller.declaredUncompressedBytes(in: "Empty zipfile."))
    }

    // MARK: - Failure paths

    func testFailureDescriptionsNameTheStep() {
        XCTAssertEqual(AppInstaller.Failure.download("x").localizedDescription, "Could not download the update: x")
        XCTAssertEqual(AppInstaller.Failure.archive("x").localizedDescription, "Could not read the update: x")
        XCTAssertEqual(AppInstaller.Failure.verification("x").localizedDescription, "The update failed verification: x")
        XCTAssertEqual(AppInstaller.Failure.install("x").localizedDescription, "Could not install the update: x")
        XCTAssertEqual(AppInstaller.Failure.notWritable.localizedDescription, "TokenMon cannot replace itself from this folder.")
    }

    func testDownloadErrorIsReportedAsADownloadFailure() async {
        ArchiveStubURLProtocol.failure = URLError(.notConnectedToInternet)
        await assertDownloadFails { failure in
            guard case .download = failure else { return XCTFail("expected download failure, got \(failure)") }
        }
    }

    func testMissingReleaseFileIsReported() async {
        ArchiveStubURLProtocol.status = 404
        await assertDownloadFails { XCTAssertEqual($0, .download("The release file was not available.")) }
    }

    func testDownloadServedOffGitHubIsRefused() async {
        ArchiveStubURLProtocol.body = Data("zip".utf8)
        ArchiveStubURLProtocol.responseURL = URL(string: "https://downloads.example.com/TokenMon.zip")
        await assertDownloadFails { XCTAssertEqual($0, .download("The release file is not hosted on GitHub.")) }
    }

    func testDeclaredLengthOverTheLimitIsRefusedBeforeReading() async {
        ArchiveStubURLProtocol.body = Data("zip".utf8)
        ArchiveStubURLProtocol.declaredLength = 500
        await assertDownloadFails(limit: 100) { XCTAssertEqual($0, .download("The release file is unexpectedly large.")) }
    }

    func testDownloadIntoAMissingFolderIsReported() async {
        ArchiveStubURLProtocol.body = Data("zip".utf8)
        let file = root.appendingPathComponent("missing/body.zip")
        await assertDownloadFails(to: file) { XCTAssertEqual($0, .download("The download could not be saved.")) }
    }

    func testInterruptedDownloadIsReported() async {
        ArchiveStubURLProtocol.body = Data(repeating: 1, count: 1024)
        ArchiveStubURLProtocol.failsAfterBody = true
        await assertDownloadFails { failure in
            guard case .download = failure else { return XCTFail("expected download failure, got \(failure)") }
        }
    }

    func testArchiveThatIsNotAZipIsRefused() async throws {
        let zip = root.appendingPathComponent("not-a.zip")
        try Data("not a zip".utf8).write(to: zip)
        do {
            _ = try await AppInstaller.extractApp(from: zip, into: root.appendingPathComponent("out", isDirectory: true))
            XCTFail("expected the archive to be refused")
        } catch let failure as AppInstaller.Failure {
            XCTAssertEqual(failure, .archive("The release zip could not be opened."))
        }
    }

    func testMissingToolIsReportedAsAnArchiveFailure() async {
        do {
            _ = try await AppInstaller.run("/nonexistent/tokenmon-tool", [])
            XCTFail("expected the launch to fail")
        } catch let failure as AppInstaller.Failure {
            guard case .archive = failure else { return XCTFail("expected archive failure, got \(failure)") }
        } catch {
            XCTFail("unexpected error \(error)")
        }
    }

    func testStagingAMissingAppIsAnInstallFailure() {
        let missing = root.appendingPathComponent("nowhere/TokenMon.app", isDirectory: true)
        XCTAssertThrowsError(try AppInstaller.stageAndSwap(missing, into: destination, expecting: expectation())) { error in
            guard case .install = error as? AppInstaller.Failure else { return XCTFail("expected install failure, got \(error)") }
        }
        XCTAssertEqual(try? bundleVersion(of: destination), "1.5.0")
    }

    /// A copy signed as TokenMon whose Info.plist names another app is refused.
    func testInfoPlistNamingAnotherAppIsRefused() throws {
        let app = try makeApp(
            in: fixtureFolder(),
            version: "2.0.0",
            bundleID: "com.example.other",
            signingIdentifier: Self.bundleID
        )
        XCTAssertThrowsError(try AppInstaller.verify(app, against: expectation())) { error in
            XCTAssertEqual(error as? AppInstaller.Failure, .verification("The download is not TokenMon."))
        }
    }

    func testQuarantinedAdHocAppIsRefused() throws {
        let app = try makeApp(in: fixtureFolder(), version: "2.0.0")
        XCTAssertFalse(AppInstaller.isQuarantined(app))
        XCTAssertEqual(try AppBundleFixture.run("/usr/bin/xattr", ["-w", "com.apple.quarantine", "0081;00000000;Test;", app.path]), 0)
        XCTAssertTrue(AppInstaller.isQuarantined(app))
        XCTAssertThrowsError(try AppInstaller.verify(app, against: expectation())) { error in
            XCTAssertEqual(error as? AppInstaller.Failure, .verification("macOS would block this unnotarized download from opening."))
        }
    }

    func testMissingAppIsNotASignedApp() {
        let missing = root.appendingPathComponent("nowhere/TokenMon.app")
        XCTAssertThrowsError(try AppInstaller.verifySignature(of: missing, identifier: Self.bundleID, teamIdentifier: nil)) { error in
            XCTAssertEqual(error as? AppInstaller.Failure, .verification("The download is not a signed app."))
        }
    }

    /// With a Team ID to match, an ad-hoc signature is not enough.
    func testTeamRequirementRefusesAnAdHocSignature() throws {
        let app = try makeApp(in: fixtureFolder(), version: "2.0.0")
        XCTAssertNil(try AppInstaller.verifySignature(of: app, identifier: Self.bundleID, teamIdentifier: nil))
        XCTAssertThrowsError(try AppInstaller.verifySignature(of: app, identifier: Self.bundleID, teamIdentifier: "ABCDE12345")) { error in
            XCTAssertEqual(error as? AppInstaller.Failure, .verification("The code signature is not valid for TokenMon."))
        }
    }

    func testIdentifierThatBreaksTheRequirementIsRefused() throws {
        let app = try makeApp(in: fixtureFolder(), version: "2.0.0")
        XCTAssertThrowsError(try AppInstaller.verifySignature(of: app, identifier: "com.\"broken", teamIdentifier: nil)) { error in
            XCTAssertEqual(error as? AppInstaller.Failure, .verification("The signing requirement could not be built."))
        }
    }

    func testRelaunchWaitsForThisProcessThenOpensTheApp() {
        let app = URL(fileURLWithPath: "/Applications/TokenMon.app")
        let process = AppInstaller.relaunchProcess(for: app, after: 4242)
        XCTAssertEqual(process.executableURL?.path, "/bin/sh")
        XCTAssertEqual(process.arguments?.count, 5)
        XCTAssertEqual(process.arguments?.first, "-c")
        let script = process.arguments?[1] ?? ""
        XCTAssertTrue(script.contains("/bin/kill -0") && script.contains("/usr/bin/open"), script)
        XCTAssertEqual(Array(process.arguments?.suffix(2) ?? []), ["4242", "/Applications/TokenMon.app"])
        XCTAssertFalse(process.isRunning)
    }

    /// The test host is ad-hoc signed (`CODE_SIGN_IDENTITY=-`), so it has no
    /// Team ID; a developer-signed run reports a ten-character one.
    func testRunningTeamIdentifierIsATeamIDOrNil() {
        let team = AppInstaller.runningTeamIdentifier()
        XCTAssertTrue(team == nil || team?.count == 10, "unexpected team \(team ?? "")")
    }

    func testRedirectsStayOnGitHubReleaseHosts() async throws {
        let delegate = TrustedReleaseRedirect()
        let task = session.dataTask(with: Self.releaseURL)
        let response = try XCTUnwrap(HTTPURLResponse(url: Self.releaseURL, statusCode: 302, httpVersion: nil, headerFields: nil))
        let cdn = URLRequest(url: URL(string: "https://objects.githubusercontent.com/release-asset/TokenMon.zip")!)
        let kept = await delegate.urlSession(session, task: task, willPerformHTTPRedirection: response, newRequest: cdn)
        XCTAssertEqual(kept?.url, cdn.url)
        let elsewhere = URLRequest(url: URL(string: "https://downloads.example.com/TokenMon.zip")!)
        let refused = await delegate.urlSession(session, task: task, willPerformHTTPRedirection: response, newRequest: elsewhere)
        XCTAssertNil(refused)
    }

    // MARK: - Helpers

    private func expectation() -> AppInstaller.Expectation {
        AppInstaller.Expectation(
            sha256: "",
            bundleIdentifier: Self.bundleID,
            currentVersion: AppVersion("1.5.0")!,
            teamIdentifier: nil
        )
    }

    private func assertDownloadFails(
        to file: URL? = nil,
        limit: Int64 = AppInstaller.maximumArchiveBytes,
        file testFile: StaticString = #filePath,
        line: UInt = #line,
        _ check: (AppInstaller.Failure) -> Void
    ) async {
        let target = file ?? root.appendingPathComponent("download-\(UUID().uuidString).zip")
        do {
            _ = try await AppInstaller.download(Self.releaseURL, to: target, session: session, limit: limit)
            XCTFail("expected the download to fail", file: testFile, line: line)
        } catch let failure as AppInstaller.Failure {
            check(failure)
        } catch {
            XCTFail("unexpected error \(error)", file: testFile, line: line)
        }
    }

    private func install(sha256: String, from url: URL = AppInstallerTests.releaseURL) async throws -> URL {
        try await AppInstaller.installUpdate(
            from: url,
            expecting: AppInstaller.Expectation(
                sha256: sha256,
                bundleIdentifier: Self.bundleID,
                currentVersion: AppVersion("1.5.0")!,
                teamIdentifier: nil
            ),
            replacing: destination,
            session: session
        )
    }

    private func assertInstallFails(
        sha256: String,
        from url: URL = AppInstallerTests.releaseURL,
        file: StaticString = #filePath,
        line: UInt = #line,
        _ check: (AppInstaller.Failure) -> Void
    ) async {
        do {
            _ = try await install(sha256: sha256, from: url)
            XCTFail("expected the install to fail", file: file, line: line)
        } catch let failure as AppInstaller.Failure {
            check(failure)
        } catch {
            XCTFail("unexpected error \(error)", file: file, line: line)
        }
        XCTAssertEqual(try? bundleVersion(of: destination), "1.5.0", "the installed app is untouched", file: file, line: line)
        let siblings = try? FileManager.default.contentsOfDirectory(atPath: destination.deletingLastPathComponent().path)
        XCTAssertEqual(siblings, ["TokenMon.app"], "no staged copy is left behind", file: file, line: line)
    }

    private func fixtureFolder() -> URL {
        let folder = root.appendingPathComponent("fixture-\(UUID().uuidString)", isDirectory: true)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder
    }

    private func makeApp(
        in folder: URL,
        version: String,
        bundleID: String = AppInstallerTests.bundleID,
        signed: Bool = true,
        signingIdentifier: String? = nil
    ) throws -> URL {
        try AppBundleFixture.makeApp(
            in: folder,
            version: version,
            bundleID: bundleID,
            signed: signed,
            signingIdentifier: signingIdentifier
        )
    }

    /// Zips `item` and serves it. Returns the zip's SHA-256.
    @discardableResult
    private func serve(_ item: URL) throws -> String {
        let zip = root.appendingPathComponent("served-\(UUID().uuidString).zip")
        ArchiveStubURLProtocol.body = try AppBundleFixture.zip(item, to: zip)
        return Self.sha256(ArchiveStubURLProtocol.body)
    }

    private func bundleVersion(of app: URL) throws -> String? {
        try AppBundleFixture.bundleVersion(of: app)
    }

    private static func sha256(_ data: Data) -> String {
        AppBundleFixture.sha256(data)
    }
}
