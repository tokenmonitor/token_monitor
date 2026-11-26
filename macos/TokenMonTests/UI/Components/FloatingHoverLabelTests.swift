import AppKit
import SwiftUI
@testable import TokenMon
import XCTest

/// The floating hover label: which item a hover phase selects, and the label
/// drawn beside the pointer while an item is hovered.
@MainActor
final class FloatingHoverLabelTests: XCTestCase {
    private final class Recorder {
        var hovered: Int?
        var labels: [Int] = []

        /// Notes that the label for `index` was built; returns `index`.
        func built(_ index: Int) -> Int {
            labels.append(index)
            return index
        }
    }

    /// Columns 50 wide: the pointer's column is the item.
    private func column(_ pointer: CGPoint, _: CGSize) -> Int? {
        pointer.x >= 0 ? Int(pointer.x / 50) : nil
    }

    func testActiveHoverSelectsTheItemUnderThePointer() {
        var sizes: [CGSize] = []
        let state = HoverLabelPlacement.hoverState(
            for: .active(CGPoint(x: 120, y: 10)),
            containerSize: CGSize(width: 200, height: 40)
        ) { pointer, size in
            sizes.append(size)
            return Int(pointer.x / 50)
        }
        XCTAssertEqual(state.pointer, CGPoint(x: 120, y: 10))
        XCTAssertEqual(state.item, 2)
        XCTAssertEqual(sizes, [CGSize(width: 200, height: 40)])
    }

    func testHoverOverNoItemKeepsThePointerButNoItem() {
        let state = HoverLabelPlacement.hoverState(for: .active(CGPoint(x: -5, y: 10)), containerSize: .zero, index: column)
        XCTAssertEqual(state.pointer, CGPoint(x: -5, y: 10))
        XCTAssertNil(state.item)
    }

    func testEndedHoverClearsPointerAndItem() {
        let state = HoverLabelPlacement.hoverState(for: .ended, containerSize: CGSize(width: 200, height: 40), index: column)
        XCTAssertNil(state.pointer)
        XCTAssertNil(state.item)
    }

    private func container(_ recorder: Recorder, pointer: CGPoint?, staysAboveBottomEdge: Bool = false) -> some View {
        VStack(spacing: 0) {
            Color.gray.frame(height: 40)
            Color.blue.frame(width: 200, height: 60)
                .floatingHoverLabel(
                    hoveredIndex: Binding(get: { recorder.hovered }, set: { recorder.hovered = $0 }),
                    staysAboveBottomEdge: staysAboveBottomEdge,
                    initialPointer: pointer,
                    index: column
                ) { index in
                    HoverLabelCard {
                        HoverLabelTitle(text: "Item \(recorder.built(index))")
                    }
                }
                .raisedWhileHoverLabelShown()
        }
    }

    /// A hovered item gets its label, built for that item.
    func testHoveredItemShowsItsLabel() {
        let recorder = Recorder()
        recorder.hovered = 2
        ViewHarness.renderBothAppearances(container(recorder, pointer: CGPoint(x: 120, y: 30)))
        XCTAssertFalse(recorder.labels.isEmpty)
        XCTAssertEqual(Set(recorder.labels), [2])
        XCTAssertEqual(recorder.hovered, 2)
    }

    /// Near the panel's bottom the label is kept inside the container.
    func testLabelThatStaysAboveTheBottomEdgeRenders() {
        let recorder = Recorder()
        recorder.hovered = 0
        let raised = ViewHarness.pixels(container(recorder, pointer: CGPoint(x: 20, y: 55), staysAboveBottomEdge: true))
        let hanging = ViewHarness.pixels(container(recorder, pointer: CGPoint(x: 20, y: 55)))
        XCTAssertEqual(Set(recorder.labels), [0])
        XCTAssertNotEqual(raised, hanging)
    }

    /// A section holding two hover containers is raised while either shows
    /// its label.
    func testSectionWithTwoHoverContainersIsRaised() {
        let first = Recorder()
        let second = Recorder()
        second.hovered = 1
        let section = VStack(spacing: 0) {
            container(first, pointer: nil)
            container(second, pointer: CGPoint(x: 60, y: 20))
        }
        .raisedWhileHoverLabelShown()
        XCTAssertGreaterThan(ViewHarness.render(section).height, 0)
        XCTAssertEqual(first.labels, [])
        XCTAssertEqual(Set(second.labels), [1])
    }

    /// Without a pointer, or without a hovered item, no label is built.
    func testNoLabelWithoutAHover() {
        let recorder = Recorder()
        ViewHarness.render(container(recorder, pointer: CGPoint(x: 120, y: 30)))
        recorder.hovered = 1
        ViewHarness.render(container(recorder, pointer: nil))
        XCTAssertEqual(recorder.labels, [])
    }
}
