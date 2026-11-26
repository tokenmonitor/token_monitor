import AppKit
@testable import TokenMon
import XCTest

/// Status-item clicks open, switch and close the dropdown, which sits under
/// the status item and closes when focus leaves it.
@MainActor
final class MenuBarControllerTests: XCTestCase {
    /// One controller for the class: each owns a real status item, which
    /// stays in the menu bar for the life of the test host.
    private static var shared: (model: AppModel, controller: MenuBarController)?

    private var model: AppModel!
    private var controller: MenuBarController!

    override func setUp() async throws {
        if Self.shared == nil {
            let model = AppModel()
            Self.shared = (model, MenuBarController(model: model))
        }
        model = Self.shared?.model
        controller = Self.shared?.controller
        if controller.isPanelVisible {
            controller.handleClick(on: nil)
        }
    }

    override func tearDown() async throws {
        if controller.isPanelVisible {
            controller.handleClick(on: nil)
        }
    }

    private func spin(_ seconds: TimeInterval = 0.1) {
        RunLoop.main.run(until: Date().addingTimeInterval(seconds))
    }

    // MARK: - Click response

    func testClosedPanelOpensOnTheClickedProvider() {
        XCTAssertEqual(MenuBarController.clickResponse(clicked: .claude, selected: .grok, isPanelVisible: false), .open(.claude))
        XCTAssertEqual(MenuBarController.clickResponse(clicked: nil, selected: .grok, isPanelVisible: false), .open(nil))
    }

    func testOpenPanelSwitchesTabsOrCloses() {
        XCTAssertEqual(MenuBarController.clickResponse(clicked: .claude, selected: .grok, isPanelVisible: true), .switchTab(.claude))
        XCTAssertEqual(MenuBarController.clickResponse(clicked: .grok, selected: .grok, isPanelVisible: true), .close)
        XCTAssertEqual(MenuBarController.clickResponse(clicked: nil, selected: .grok, isPanelVisible: true), .close)
    }

    // MARK: - Panel frame

    private let visible = NSRect(x: 0, y: 0, width: 1440, height: 875)

    /// The panel hangs centered under the status item, 4pt below it.
    func testPanelHangsCenteredUnderTheStatusItem() {
        let frame = MenuBarController.panelFrame(
            for: NSSize(width: 420, height: 480),
            under: NSRect(x: 700, y: 800, width: 30, height: 22),
            in: visible,
            gap: 4
        )
        XCTAssertEqual(frame, NSRect(x: 505, y: 316, width: 420, height: 480))
    }

    /// A status item at the top of the screen keeps the panel's top 8pt
    /// under the visible area.
    func testPanelTopStaysInsideTheVisibleArea() {
        let frame = MenuBarController.panelFrame(
            for: NSSize(width: 420, height: 480),
            under: NSRect(x: 700, y: 878, width: 30, height: 22),
            in: visible,
            gap: 4
        )
        XCTAssertEqual(frame.maxY, 875 - 8)
    }

    /// Near either screen edge the panel stays 8pt inside it.
    func testPanelStaysInsideTheScreenSides() {
        let size = NSSize(width: 420, height: 480)
        let right = MenuBarController.panelFrame(for: size, under: NSRect(x: 1420, y: 880, width: 20, height: 22), in: visible, gap: 4)
        XCTAssertEqual(right.minX, 1440 - 420 - 8)
        let left = MenuBarController.panelFrame(for: size, under: NSRect(x: 0, y: 880, width: 20, height: 22), in: visible, gap: 4)
        XCTAssertEqual(left.minX, 8)
    }

    /// Content taller than the screen is cut to the visible height less
    /// 16pt and kept 8pt above the bottom.
    func testTallPanelIsClampedToTheScreen() {
        let frame = MenuBarController.panelFrame(
            for: NSSize(width: 420, height: 2_000),
            under: NSRect(x: 700, y: 880, width: 30, height: 22),
            in: visible,
            gap: 4
        )
        XCTAssertEqual(frame.height, 875 - 16)
        XCTAssertEqual(frame.minY, 8)
    }

    // MARK: - Live panel

    func testClicksOpenSwitchAndCloseThePanel() {
        model.settings.selectedProvider = .overview
        controller.handleClick(on: .claude)
        XCTAssertTrue(controller.isPanelVisible)
        XCTAssertEqual(model.settings.selectedProvider, .claude)
        XCTAssertTrue(model.claudePoller.menuIsOpen)
        spin()

        controller.handleClick(on: .grok)
        XCTAssertTrue(controller.isPanelVisible, "another provider's bar switches tabs in place")
        XCTAssertEqual(model.settings.selectedProvider, .grok)
        spin()

        controller.handleClick(on: .grok)
        XCTAssertFalse(controller.isPanelVisible)
        XCTAssertFalse(model.claudePoller.menuIsOpen)
    }

    /// Switching to a taller tab re-fits the open panel to its content.
    func testPanelRefitsWhenTheTabChanges() throws {
        model.settings.selectedProvider = .grok
        controller.handleClick(on: nil)
        spin(0.2)
        let panel = try XCTUnwrap(NSApp.windows.first { $0 is MenuBarPanel && $0.isVisible })
        let grokHeight = panel.frame.height
        controller.handleClick(on: .overview)
        spin(0.3)
        XCTAssertTrue(controller.isPanelVisible)
        XCTAssertNotEqual(panel.frame.height, grokHeight)
        // The width is the content's, plus the scroll bar's when the screen clamps the height
        // and the scroll bar style is the always-visible one.
        XCTAssertEqual(panel.frame.width, MenuBarController.panelWidth(isHeightClamped: controller.isPanelHeightClamped))
    }

    /// An overlay scroll bar floats over the content; only a visible legacy one widens the panel.
    func testPanelWidthGrowsOnlyForAVisibleLegacyScrollBar() {
        let legacyBar = NSScroller.scrollerWidth(for: .regular, scrollerStyle: .legacy)
        XCTAssertEqual(MenuBarController.panelWidth(isHeightClamped: false, scrollerStyle: .legacy), MenuBarPanelView.panelWidth)
        XCTAssertEqual(MenuBarController.panelWidth(isHeightClamped: true, scrollerStyle: .overlay), MenuBarPanelView.panelWidth)
        XCTAssertEqual(
            MenuBarController.panelWidth(isHeightClamped: true, scrollerStyle: .legacy),
            MenuBarPanelView.panelWidth + legacyBar
        )
        XCTAssertGreaterThan(legacyBar, 0)
    }

    /// A click on the status item itself always opens a closed panel,
    /// whichever bar the pointer is over.
    func testStatusItemClickOpensThePanel() {
        controller.perform(Selector(("handleStatusItemClick:")), with: nil)
        XCTAssertTrue(controller.isPanelVisible)
    }

    /// A miss opens the panel on the current selection.
    func testMissOpensOnTheCurrentSelection() {
        model.settings.selectedProvider = .cursor
        controller.handleClick(on: nil)
        XCTAssertTrue(controller.isPanelVisible)
        XCTAssertEqual(model.settings.selectedProvider, .cursor)
    }

    func testPanelClosesWhenItLosesFocus() {
        controller.handleClick(on: nil)
        XCTAssertTrue(controller.isPanelVisible)
        let panel = NSApp.windows.first { $0 is MenuBarPanel && $0.isVisible }
        XCTAssertNotNil(panel)
        NotificationCenter.default.post(name: NSWindow.didResignKeyNotification, object: panel)
        XCTAssertFalse(controller.isPanelVisible)
    }

    func testPanelClosesWhenTheAppResignsActive() {
        controller.handleClick(on: nil)
        XCTAssertTrue(controller.isPanelVisible)
        NotificationCenter.default.post(name: NSApplication.didResignActiveNotification, object: NSApp)
        XCTAssertFalse(controller.isPanelVisible)
        // Closing an already closed panel changes nothing.
        NotificationCenter.default.post(name: NSApplication.didResignActiveNotification, object: NSApp)
        XCTAssertFalse(controller.isPanelVisible)
    }

    /// Escape closes the open panel and is swallowed; other keys pass through.
    func testEscapeClosesThePanel() throws {
        controller.handleClick(on: nil)
        XCTAssertTrue(controller.isPanelVisible)
        for keyCode: UInt16 in [0, 53] {
            let event = try XCTUnwrap(NSEvent.keyEvent(
                with: .keyDown,
                location: .zero,
                modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: 0,
                context: nil,
                characters: keyCode == 53 ? "\u{1B}" : "a",
                charactersIgnoringModifiers: keyCode == 53 ? "\u{1B}" : "a",
                isARepeat: false,
                keyCode: keyCode
            ))
            NSApp.postEvent(event, atStart: true)
            // The app's event dispatch runs the local key monitor.
            let next = try XCTUnwrap(NSApp.nextEvent(
                matching: .keyDown,
                until: Date().addingTimeInterval(0.2),
                inMode: .default,
                dequeue: true
            ))
            NSApp.sendEvent(next)
            XCTAssertEqual(controller.isPanelVisible, keyCode != 53, "key \(keyCode)")
        }
    }
}
