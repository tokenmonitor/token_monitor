import AppKit
@testable import TokenMon
import XCTest

/// Sleep pauses polling; wake resumes it only once the network path is
/// satisfied again, or after the fallback delay when the path never changes.
@MainActor
final class SystemWakeGateTests: XCTestCase {
    private var events: [String] = []
    private var scheduler: ManualScheduler!

    override func setUp() async throws {
        events = []
        scheduler = ManualScheduler()
    }

    private func makeGate() -> SystemWakeGate {
        let scheduler = scheduler!
        return SystemWakeGate(
            observeSystem: false,
            fallbackDelay: 30,
            sleep: { seconds in try await scheduler.sleep(seconds) },
            onSleep: { [weak self] in self?.events.append("sleep") },
            onReady: { [weak self] in self?.events.append("ready") }
        )
    }

    func testWakeWaitsForASatisfiedPath() async {
        let gate = makeGate()
        gate.systemWillSleep()
        gate.systemDidWake()
        XCTAssertEqual(events, ["sleep"])
        gate.pathChanged(satisfied: false)
        XCTAssertEqual(events, ["sleep"])
        gate.pathChanged(satisfied: true)
        XCTAssertEqual(events, ["sleep", "ready"])
        await scheduler.settle()
        XCTAssertEqual(scheduler.pendingCount, 0, "the fallback wait is cancelled")
    }

    func testPathThatCameBackDuringWakeResumesImmediately() {
        let gate = makeGate()
        gate.systemWillSleep()
        gate.pathChanged(satisfied: true)
        gate.systemDidWake()
        XCTAssertEqual(events, ["sleep", "ready"])
    }

    func testFallbackResumesWhenThePathNeverChanges() async throws {
        let gate = makeGate()
        gate.systemWillSleep()
        gate.systemDidWake()
        let delay = try await scheduler.nextSleep()
        XCTAssertEqual(delay, 30)
        scheduler.fire()
        await scheduler.settle()
        XCTAssertEqual(events, ["sleep", "ready"])
    }

    func testPathChangesOutsideAWakeDoNothing() {
        let gate = makeGate()
        gate.pathChanged(satisfied: false)
        gate.pathChanged(satisfied: true)
        XCTAssertEqual(events, [])
    }

    // MARK: - System observation

    /// Workspace sleep and wake notifications drive the gate, and a released
    /// gate stops listening.
    func testWorkspaceNotificationsPauseAndResume() async throws {
        let center = NotificationCenter()
        var gate: SystemWakeGate? = SystemWakeGate(
            workspaceCenter: center,
            fallbackDelay: 0,
            sleep: { _ in },
            onSleep: { [weak self] in self?.events.append("sleep") },
            onReady: { [weak self] in self?.events.append("ready") }
        )
        center.post(name: NSWorkspace.willSleepNotification, object: nil)
        await waitUntil { self.events == ["sleep"] }
        center.post(name: NSWorkspace.didWakeNotification, object: nil)
        await waitUntil { self.events == ["sleep", "ready"] }
        XCTAssertEqual(gate?.isAwaitingNetwork, false)

        gate = nil
        center.post(name: NSWorkspace.willSleepNotification, object: nil)
        center.post(name: NSWorkspace.didWakeNotification, object: nil)
        try await Task.sleep(nanoseconds: 50_000_000)
        XCTAssertEqual(events, ["sleep", "ready"], "a released gate no longer observes")
    }

    /// The live path monitor's first report ends a wake that is waiting for it.
    func testPathMonitorReportEndsAWait() async {
        let gate = SystemWakeGate(
            workspaceCenter: NotificationCenter(),
            fallbackDelay: 3600,
            sleep: { _ in try await Task.sleep(nanoseconds: .max) },
            onSleep: {},
            onReady: { [weak self] in self?.events.append("ready") }
        )
        gate.systemDidWake()
        await waitUntil(timeout: 5) { !self.events.isEmpty || !gate.isNetworkSatisfied }
        if gate.isNetworkSatisfied {
            XCTAssertEqual(events, ["ready"])
            XCTAssertFalse(gate.isAwaitingNetwork)
        } else {
            XCTAssertTrue(gate.isAwaitingNetwork, "an offline path keeps the wake waiting")
        }
    }

    /// The default sleeper waits the fallback delay, then resumes.
    func testDefaultSleeperRunsTheFallback() async {
        let gate = SystemWakeGate(
            observeSystem: false,
            fallbackDelay: 0,
            onSleep: {},
            onReady: { [weak self] in self?.events.append("ready") }
        )
        gate.systemWillSleep()
        gate.systemDidWake()
        await waitUntil { self.events == ["ready"] }
        XCTAssertFalse(gate.isAwaitingNetwork)
    }

    private func waitUntil(timeout: TimeInterval = 2, _ condition: () -> Bool) async {
        let deadline = Date().addingTimeInterval(timeout)
        while !condition(), Date() < deadline {
            try? await Task.sleep(nanoseconds: 5_000_000)
        }
        XCTAssertTrue(condition(), "timed out; events \(events)")
    }
}
