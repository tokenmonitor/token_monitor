import AppKit
import SwiftUI
import XCTest

/// Renders a SwiftUI view the way the app does.
///
/// `render` hosts the view in an offscreen `NSHostingView`, lays it out and
/// draws it, so SwiftUI evaluates every body, `GeometryReader` and `ForEach`
/// the real panel would. Pair it with `ViewTree` to assert on what the view
/// shows and to run its buttons.
@MainActor
enum ViewHarness {
    /// Hosts `view` at `width`, lays it out and draws it into a bitmap.
    /// Returns the fitted size, so a test can compare states by height.
    @discardableResult
    static func render(_ view: some View, width: CGFloat = 340, colorScheme: ColorScheme = .light) -> CGSize {
        let host = NSHostingView(rootView: view.environment(\.colorScheme, colorScheme).frame(width: width))
        let fitting = host.fittingSize
        host.frame = NSRect(x: 0, y: 0, width: width, height: max(1, min(fitting.height, 4000)))
        host.layoutSubtreeIfNeeded()
        if let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) {
            host.cacheDisplay(in: host.bounds, to: rep)
        }
        return fitting
    }

    /// The size `view` asks for with no width imposed, for comparing the
    /// natural widths of compact views (pills, labels).
    static func idealSize(_ view: some View) -> CGSize {
        NSHostingView(rootView: view.fixedSize()).fittingSize
    }

    /// The pixels of `view` drawn into a `size` frame, so a test can tell two
    /// states apart that show the same strings (a hover outline, a label
    /// placed elsewhere). Lays out twice so measured sizes settle.
    static func pixels(_ view: some View, size: CGSize = CGSize(width: 340, height: 120)) -> Data? {
        let host = NSHostingView(rootView: view.frame(width: size.width, height: size.height))
        host.frame = NSRect(origin: .zero, size: size)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        host.layoutSubtreeIfNeeded()
        guard let rep = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { return nil }
        host.cacheDisplay(in: host.bounds, to: rep)
        return rep.tiffRepresentation
    }

    /// Renders `view` in dark, then light appearance; returns the light size.
    @discardableResult
    static func renderBothAppearances(_ view: some View, width: CGFloat = 340) -> CGSize {
        render(view, width: width, colorScheme: .dark)
        return render(view, width: width, colorScheme: .light)
    }
}

/// The strings and buttons a SwiftUI view value contains, found with `Mirror`.
///
/// The walk expands custom views by calling `body` on a copy outside the
/// render graph, so `@State` reads its initial value and `@Environment` its
/// default; tests that need another state drive it through the models. It
/// does not descend into reference types outside SwiftUI (models, sessions),
/// only into the view values and SwiftUI's own storage.
@MainActor
struct ViewTree {
    /// A button found in the tree: the strings in its label and its action.
    struct Button {
        let label: [String]
        let action: () -> Void
    }

    /// A toggle found in the tree: the strings in its label and its binding.
    struct Toggle {
        let label: [String]
        let isOn: Binding<Bool>
    }

    private(set) var texts: [String] = []
    private(set) var buttons: [Button] = []
    private(set) var toggles: [Toggle] = []
    /// Every custom (non-primitive) view value met during the walk.
    private(set) var views: [Any] = []

    init(_ view: some View, maxDepth: Int = 120) {
        walk(view, depth: maxDepth)
    }

    private init() {}

    /// True when a shown string contains `text`.
    func shows(_ text: String) -> Bool {
        texts.contains { $0.contains(text) }
    }

    /// Custom views of type `V` in the tree, in walk order; a test reads their
    /// stored closures to check how a parent wired them.
    func all<V: View>(_ type: V.Type) -> [V] {
        views.compactMap { $0 as? V }
    }

    /// The first custom view of type `V`; fails the test when there is none.
    func first<V: View>(_ type: V.Type, file: StaticString = #filePath, line: UInt = #line) -> V? {
        guard let view = all(type).first else {
            XCTFail("no \(V.self) in the tree", file: file, line: line)
            return nil
        }
        return view
    }

    /// Runs the first button whose label contains `label`.
    func press(_ label: String, file: StaticString = #filePath, line: UInt = #line) {
        guard let button = buttons.first(where: { $0.label.contains { $0.contains(label) } }) else {
            XCTFail("no button labelled \(label); buttons: \(buttons.map(\.label))", file: file, line: line)
            return
        }
        button.action()
    }

    /// The binding of the first toggle whose label contains `label`; a test
    /// reads or sets it as flipping the switch would.
    func toggle(_ label: String, file: StaticString = #filePath, line: UInt = #line) -> Binding<Bool>? {
        guard let toggle = toggles.first(where: { $0.label.contains { $0.contains(label) } }) else {
            XCTFail("no toggle labelled \(label); toggles: \(toggles.map(\.label))", file: file, line: line)
            return nil
        }
        return toggle.isOn
    }

    private mutating func walk(_ value: Any, depth: Int) {
        guard depth > 0 else { return }
        if let string = value as? String {
            if texts.last != string {
                texts.append(string)
            }
            return
        }
        let typeName = String(describing: type(of: value))
        if typeName.hasPrefix("Button<"), let action = Self.action(of: value) {
            var label = ViewTree()
            label.walkChildren(of: value, depth: depth)
            buttons.append(Button(label: label.texts, action: action))
            absorb(label)
            return
        }
        if typeName.hasPrefix("Toggle<"), let binding = Self.boolBinding(of: value) {
            var label = ViewTree()
            label.walkChildren(of: value, depth: depth)
            toggles.append(Toggle(label: label.texts, isOn: binding))
            absorb(label)
            return
        }
        if typeName.hasPrefix("ModifiedContent<") {
            walkModified(value, depth: depth)
            return
        }
        if let forEach = value as? any ViewTreeRows {
            for row in forEach.rows {
                walk(row, depth: depth - 1)
            }
            return
        }
        if Mirror(reflecting: value).displayStyle == .class, !String(reflecting: type(of: value)).hasPrefix("SwiftUI.") {
            return
        }
        if let view = value as? any View, let body = Self.body(of: view) {
            views.append(view)
            walk(body, depth: depth - 1)
            return
        }
        // A bodiless view the app declares (an AppKit representable) is
        // recorded too, so a test can read how its parent configured it.
        // Library wrappers such as `Optional<V>` are not: they would cast to
        // the wrapped view and count it twice.
        if let view = value as? any View, String(reflecting: type(of: value)).hasPrefix("TokenMon.") {
            views.append(view)
        }
        walkChildren(of: value, depth: depth)
    }

    /// Walks a modified view; strings in its modifiers (an accessibility
    /// label, a help tag) join the label of the one button it wraps.
    private mutating func walkModified(_ value: Any, depth: Int) {
        let children = Mirror(reflecting: value).children
        let before = buttons.count
        for child in children where child.label == "content" {
            walk(child.value, depth: depth - 1)
        }
        var modifier = ViewTree()
        for child in children where child.label != "content" {
            modifier.walk(child.value, depth: depth - 1)
        }
        if buttons.count == before + 1, let button = buttons.last {
            buttons[before] = Button(label: button.label + modifier.texts, action: button.action)
        }
        absorb(modifier)
    }

    private mutating func absorb(_ other: ViewTree) {
        for text in other.texts where texts.last != text {
            texts.append(text)
        }
        buttons += other.buttons
        toggles += other.toggles
        views += other.views
    }

    private mutating func walkChildren(of value: Any, depth: Int) {
        for child in Mirror(reflecting: value).children {
            walk(child.value, depth: depth - 1)
        }
    }

    private static func action(of value: Any, depth: Int = 6) -> (() -> Void)? {
        guard depth > 0 else { return nil }
        for child in Mirror(reflecting: value).children {
            if let action = child.value as? () -> Void {
                return action
            }
            if let action = child.value as? @MainActor () -> Void {
                return { MainActor.assumeIsolated { action() } }
            }
            if let found = action(of: child.value, depth: depth - 1) {
                return found
            }
        }
        return nil
    }

    /// A toggle keeps its `isOn` binding as a `Binding<ToggleState>` whose
    /// location wraps the original binding. A `Binding(get:set:)` (the kind
    /// app code writes) ends in a get/set pair, rebuilt here; a key-path
    /// binding such as `$settings.flag` has none and yields nil.
    private static func boolBinding(of value: Any, depth: Int = 10) -> Binding<Bool>? {
        guard depth > 0 else { return nil }
        let children = Array(Mirror(reflecting: value).children)
        if let get = children.first(where: { $0.label == "getValue" })?.value as? () -> Bool,
           let set = children.first(where: { $0.label == "setValue" })?.value as? (Bool, Transaction) -> Void {
            return Binding(get: get, set: { set($0, Transaction()) })
        }
        for child in children {
            if let binding = child.value as? Binding<Bool> {
                return binding
            }
            if let found = boolBinding(of: child.value, depth: depth - 1) {
                return found
            }
        }
        return nil
    }

    private static func body<V: View>(of view: V) -> Any? {
        V.Body.self == Never.self ? nil : view.body
    }
}

/// A `ForEach` whose rows the walk builds by calling its content closure on
/// each element, so a list's rows are walked like any other child.
private protocol ViewTreeRows {
    var rows: [Any] { get }
}

extension ForEach: ViewTreeRows where Content: View {
    fileprivate var rows: [Any] {
        data.map { content($0) }
    }
}
