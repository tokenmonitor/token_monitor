import CryptoKit
import Foundation
import XCTest

/// Builds small `TokenMon.app` bundles and release zips in a temporary folder
/// for the update tests. Nothing touches /Applications or the network.
enum AppBundleFixture {
    static let bundleID = "com.modelmonitor.app"

    /// Builds `TokenMon.app` in `folder` around a copy of `/usr/bin/true`,
    /// ad-hoc signed unless `signed` is false. `signingIdentifier` overrides
    /// the identifier codesign records (it defaults to `bundleID`).
    static func makeApp(
        in folder: URL,
        version: String,
        bundleID: String = AppBundleFixture.bundleID,
        signed: Bool = true,
        signingIdentifier: String? = nil
    ) throws -> URL {
        let app = folder.appendingPathComponent("TokenMon.app", isDirectory: true)
        let contents = app.appendingPathComponent("Contents", isDirectory: true)
        let macOS = contents.appendingPathComponent("MacOS", isDirectory: true)
        let resources = contents.appendingPathComponent("Resources", isDirectory: true)
        try FileManager.default.createDirectory(at: macOS, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: resources, withIntermediateDirectories: true)
        try FileManager.default.copyItem(at: URL(fileURLWithPath: "/usr/bin/true"), to: macOS.appendingPathComponent("TokenMon"))
        try Data("fixture".utf8).write(to: resources.appendingPathComponent("data.txt"))
        let info: [String: Any] = [
            "CFBundleIdentifier": bundleID,
            "CFBundleExecutable": "TokenMon",
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": version,
            "CFBundleVersion": "1"
        ]
        let plist = try PropertyListSerialization.data(fromPropertyList: info, format: .xml, options: 0)
        try plist.write(to: contents.appendingPathComponent("Info.plist"))
        if signed {
            var arguments = ["--force", "--sign", "-"]
            if let signingIdentifier {
                arguments += ["--identifier", signingIdentifier]
            }
            XCTAssertEqual(try run("/usr/bin/codesign", arguments + [app.path]), 0, "codesign")
        }
        return app
    }

    /// Zips `item` into `zip`: an `.app` as the release does, a folder as its
    /// contents. Returns the zip's bytes.
    static func zip(_ item: URL, to zip: URL) throws -> Data {
        let arguments = item.pathExtension == "app"
            ? ["-c", "-k", "--keepParent", item.path, zip.path]
            : ["-c", "-k", item.path, zip.path]
        XCTAssertEqual(try run("/usr/bin/ditto", arguments), 0, "ditto")
        return try Data(contentsOf: zip)
    }

    static func bundleVersion(of app: URL) throws -> String? {
        let data = try Data(contentsOf: app.appendingPathComponent("Contents/Info.plist"))
        let info = try PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any]
        return info?["CFBundleShortVersionString"] as? String
    }

    static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    @discardableResult
    static func run(_ executable: String, _ arguments: [String]) throws -> Int32 {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        try process.run()
        process.waitUntilExit()
        return process.terminationStatus
    }
}
