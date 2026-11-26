@testable import TokenMon
import XCTest

/// FileBackedStringStore round trips values in `0600` files and reports
/// every I/O failure as a missing value or a `false` write instead of
/// throwing or crashing.
final class FileBackedStringStoreTests: XCTestCase {
    private var dir: URL!

    override func setUpWithError() throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: dir.path)
        try? FileManager.default.removeItem(at: dir)
    }

    func testValuesRoundTripInOwnerOnlyFiles() throws {
        let store = FileBackedStringStore(directory: dir, filenamePrefix: "t_")
        XCTAssertNil(store.value(forKey: "k"))
        XCTAssertTrue(store.set("hello", forKey: "k"))
        XCTAssertEqual(store.value(forKey: "k"), "hello")
        let attributes = try FileManager.default.attributesOfItem(atPath: dir.appendingPathComponent("t_k.dat").path)
        XCTAssertEqual((attributes[.posixPermissions] as? NSNumber)?.intValue, 0o600)
        store.remove(forKey: "k")
        XCTAssertNil(store.value(forKey: "k"))
        store.remove(forKey: "k")
    }

    /// A key path that is a directory cannot be read, replaced or removed as
    /// a file; each operation fails quietly.
    func testUnreadableOrUnreplaceableKeyFailsQuietly() throws {
        let store = FileBackedStringStore(directory: dir, filenamePrefix: "t_")
        let blocker = dir.appendingPathComponent("t_k.dat", isDirectory: true)
        try FileManager.default.createDirectory(at: blocker.appendingPathComponent("child"), withIntermediateDirectories: true)

        XCTAssertNil(store.value(forKey: "k"))
        XCTAssertFalse(store.set("v", forKey: "k"), "rename onto a non-empty directory fails")
        let leftovers = try FileManager.default.contentsOfDirectory(atPath: dir.path).filter { $0.hasSuffix(".tmp") }
        XCTAssertEqual(leftovers, [], "the staging file is cleaned up")
    }

    func testWriteIntoAReadOnlyDirectoryFails() throws {
        let store = FileBackedStringStore(directory: dir, filenamePrefix: "t_")
        XCTAssertTrue(store.set("v", forKey: "k"))
        try FileManager.default.setAttributes([.posixPermissions: 0o500], ofItemAtPath: dir.path)
        XCTAssertFalse(store.set("w", forKey: "k"))
        store.remove(forKey: "k")
        XCTAssertEqual(store.value(forKey: "k"), "v", "a refused remove keeps the value")
    }
}
