import SwiftUI
@testable import TokenMon
import XCTest

/// The view-tree helper the screen tests lean on: it reads a toggle's binding
/// whatever way this release of SwiftUI wraps it.
@MainActor
final class ViewTreeTests: XCTestCase {
    /// A toggle built with `Binding(get:set:)` is found by its label, and its
    /// binding reads and writes the underlying value.
    func testToggleBindingReadsAndWritesTheUnderlyingValue() throws {
        var flag = false
        let view = Toggle(isOn: Binding(get: { flag }, set: { flag = $0 })) { Text("Alpha") }

        let binding = try XCTUnwrap(ViewTree(view).toggle("Alpha"))

        XCTAssertFalse(binding.wrappedValue)
        binding.wrappedValue = true
        XCTAssertTrue(flag)
        XCTAssertTrue(binding.wrappedValue)
        binding.wrappedValue = false
        XCTAssertFalse(flag)
        XCTAssertFalse(binding.wrappedValue)
    }
}
