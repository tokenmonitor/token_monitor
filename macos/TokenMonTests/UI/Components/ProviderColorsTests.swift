import AppKit
import SwiftUI
@testable import TokenMon
import XCTest

/// Every provider has its own brand accent, shared by the panels and the menu bar.
final class ProviderColorsTests: XCTestCase {
    private func components(_ provider: MonitorProvider) -> [Double] {
        let accent = provider.accent
        return [accent.red, accent.green, accent.blue, accent.alpha]
    }

    func testEachProviderHasItsBrandAccent() {
        XCTAssertEqual(components(.overview), [0.5, 0.5, 0.5, 1])
        XCTAssertEqual(components(.grok), [0.11, 0.38, 0.82, 1])
        XCTAssertEqual(components(.opencode), [1.0, 0.55, 0.0, 1])
        XCTAssertEqual(components(.cursor), [0.18, 0.53, 0.38, 1])
        XCTAssertEqual(components(.claude), [0.85, 0.47, 0.34, 1])
        XCTAssertEqual(components(.chatgpt), [0.16, 0.52, 0.46, 1])
        XCTAssertEqual(components(.openrouter), [0.45, 0.36, 0.90, 1])
        XCTAssertEqual(components(.grokbot), [0.38, 0.42, 0.50, 1])
    }

    /// No two providers share an accent, so the menu bar segments stay apart.
    func testProviderAccentsAreDistinct() {
        let accents = MonitorProvider.allCases.map { components($0).map { Int(($0 * 100).rounded()) } }
        XCTAssertEqual(Set(accents).count, accents.count)
    }

    /// The panels' accent is the brand colour at 85% opacity.
    func testPanelAccentIsTheBrandColourSlightlyTranslucent() throws {
        for provider in MonitorProvider.allCases {
            let resolved = try XCTUnwrap(NSColor(provider.accentColor).usingColorSpace(.sRGB), provider.rawValue)
            let accent = provider.accent
            XCTAssertEqual(resolved.alphaComponent, 0.85, accuracy: 0.01, provider.rawValue)
            XCTAssertEqual(resolved.redComponent, accent.red, accuracy: 0.01, provider.rawValue)
            XCTAssertEqual(resolved.greenComponent, accent.green, accuracy: 0.01, provider.rawValue)
            XCTAssertEqual(resolved.blueComponent, accent.blue, accuracy: 0.01, provider.rawValue)
        }
    }
}
