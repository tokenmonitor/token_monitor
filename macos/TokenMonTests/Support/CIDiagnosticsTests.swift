import AppKit
import SwiftUI
@testable import TokenMon
import XCTest

/// Temporary: prints facts about the CI runner (removed before merge).
@MainActor
final class CIDiagnosticsTests: XCTestCase {
    /// XCTest failures reach the CI log; stdout of the test host does not.
    private func report(_ items: Any...) {
        XCTFail("DIAG " + items.map { "\($0)" }.joined(separator: " "))
    }

    private func spin(_ seconds: TimeInterval = 0.2) {
        RunLoop.main.run(until: Date().addingTimeInterval(seconds))
    }

    private func dumpMirror(_ value: Any, label: String, depth: Int, indent: String) {
        let type = String(describing: Swift.type(of: value)).prefix(110)
        let leaf: String
        if value is String || value is Int || value is Double || value is Bool { leaf = " = \(value)" } else { leaf = "" }
        report("T \(indent)\(label): \(type)\(leaf)")
        guard depth > 0 else { return }
        for child in Mirror(reflecting: value).children {
            dumpMirror(child.value, label: child.label ?? "_", depth: depth - 1, indent: indent + " ")
        }
    }

    func testPanelAndScrollerFacts() throws {
        report("scrollerStyle", NSScroller.preferredScrollerStyle.rawValue,
               "legacyWidth", NSScroller.scrollerWidth(for: .regular, scrollerStyle: .legacy),
               "overlayWidth", NSScroller.scrollerWidth(for: .regular, scrollerStyle: .overlay))
        report("screens", NSScreen.screens.map(\.visibleFrame))
        let model = AppModel()
        let controller = MenuBarController(model: model)
        model.settings.selectedProvider = .grok
        controller.handleClick(on: nil)
        spin()
        let panel = try XCTUnwrap(NSApp.windows.first { $0 is MenuBarPanel && $0.isVisible })
        func facts(_ name: String) {
            let vc = panel.contentViewController
            report("\(name) frame", panel.frame, "contentView", panel.contentView?.frame as Any,
                   "preferred", vc?.preferredContentSize as Any, "fitting", vc?.view.fittingSize as Any)
        }
        facts("grok")
        controller.handleClick(on: .overview)
        spin(0.4)
        facts("overview")
        controller.handleClick(on: nil)
        report("ideal MenuBarRoot", ViewHarness.idealSize(MenuBarRoot(model: model)))
    }

    func testToggleStructure() {
        var flag = true
        let toggle = Toggle(isOn: Binding(get: { flag }, set: { flag = $0 })) { Text("Alpha") }
        dumpMirror(toggle, label: "toggle", depth: 12, indent: "")
        let tree = ViewTree(toggle)
        report("ViewTree toggles:", tree.toggles.map(\.label))
    }
}
