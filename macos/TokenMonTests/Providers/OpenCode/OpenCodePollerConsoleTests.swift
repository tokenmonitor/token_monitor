@testable import TokenMon
import XCTest

/// The OpenCode poller's console path (official Go usage, merged with the
/// local model breakdown) and the local-estimate messages for each way the
/// console can be unavailable. Every poller reads a database that does not
/// exist, so nothing touches the real `~/.local/share/opencode`.
@MainActor
final class OpenCodePollerConsoleTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var settings: AppSettings!
    private var auth: OpenCodeAuthSession!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "OpenCodeConsole-\(UUID().uuidString)"
        settings = AppSettings(defaults: try XCTUnwrap(UserDefaults(suiteName: suiteName)))
        settings.selectedProvider = .opencode
        auth = OpenCodeAuthSession(directory: dir)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        UserDefaults(suiteName: suiteName)?.removePersistentDomain(forName: suiteName)
    }

    private var missingDatabase: URL { dir.appendingPathComponent("opencode.db") }

    private func consoleSnapshot(monthlyUSD: Double = 15) -> OpenCodeSnapshot {
        OpenCodeSnapshot(
            windows: [
                OpenCodeWindowUsage(kind: .rolling5h, usedUSD: 1, limitUSD: 12, resetsAt: Date().addingTimeInterval(3_600), sessionCount: 0),
                OpenCodeWindowUsage(kind: .monthly, usedUSD: monthlyUSD, limitUSD: 60, resetsAt: Date().addingTimeInterval(20 * 86_400), sessionCount: 0)
            ],
            models: [],
            isEstimated: false
        )
    }

    private let localModels = [OpenCodeModelUsage(
        providerID: "opencode-go",
        modelID: "kimi-k3",
        sessionCount: 2,
        inputTokens: 100,
        outputTokens: 20,
        cacheReadTokens: 0,
        cacheWriteTokens: 0,
        costUSD: 1.5,
        percentOfWindow: 10
    )]

    private func makePoller(
        console: @escaping (String, String?) async throws -> (OpenCodeSnapshot, String),
        local: (() async throws -> (OpenCodeSnapshot?, OpenCodeDayHourlyUsage?))? = nil
    ) -> OpenCodeUsagePoller {
        OpenCodeUsagePoller(
            settings: settings,
            auth: auth,
            fetchConsole: console,
            fetchLocal: local,
            databaseURL: missingDatabase
        )
    }

    func testConsoleUsageMergesLocalModelsAndSavesTheWorkspace() async throws {
        auth.save(cookieHeader: "auth=live")
        var knownOrg: String?? = .none
        let local = OpenCodeSnapshot(windows: [], models: localModels, monthlyTokens: 120, monthlyEstimatedUSD: 1.5)
        let poller = makePoller(
            console: { _, org in
                knownOrg = .some(org)
                return (self.consoleSnapshot(), "wrk_1")
            },
            local: { (local, nil) }
        )

        await poller.refreshNow()

        XCTAssertEqual(knownOrg, .some(nil), "no workspace is known before the first success")
        XCTAssertEqual(auth.workspaceID, "wrk_1")
        let snapshot = try XCTUnwrap(poller.snapshot)
        XCTAssertFalse(snapshot.isEstimated)
        XCTAssertEqual(snapshot.models.map(\.modelID), ["kimi-k3"])
        XCTAssertEqual(snapshot.monthlyTokens, 120)
        XCTAssertEqual(poller.dataSourceLabel, "OpenCode console")
        XCTAssertNil(poller.lastError)
        XCTAssertEqual(poller.headlineUsedPercent ?? -1, 25, accuracy: 0.001)
        XCTAssertNil(poller.headlineResetsAt)
        XCTAssertFalse(auth.needsSignIn)

        await poller.refreshNow()
        XCTAssertEqual(knownOrg, .some("wrk_1"), "the saved workspace is tried first")
    }

    /// Without local rows the bars spread the console's monthly total over
    /// the elapsed days, and earlier weeks re-slice the same month.
    func testConsoleTotalShapesTheDailyBarsWithoutLocalRows() async throws {
        auth.save(cookieHeader: "auth=live")
        let poller = makePoller(console: { _, _ in (self.consoleSnapshot(monthlyUSD: 30), "wrk") }, local: { (nil, nil) })
        await poller.refreshNow()
        let days = try XCTUnwrap(poller.dailyBudgetDays)
        XCTAssertEqual(days.count, 7)
        XCTAssertNotNil(poller.dailyBudgetPeriodStart)
        XCTAssertEqual(poller.dailyBudgetDays(weekOffset: 0), days)
        XCTAssertNotNil(poller.dailyBudgetDays(weekOffset: -1))
    }

    func testCancelledConsoleFetchPublishesNothing() async {
        auth.save(cookieHeader: "auth=live")
        let poller = makePoller(console: { _, _ in throw CancellationError() }, local: { (self.consoleSnapshot(), nil) })
        await poller.refreshNow()
        XCTAssertNil(poller.snapshot)
        XCTAssertNil(poller.lastError)
    }

    func testNetworkFailureFallsBackToTheLabelledLocalEstimate() async {
        auth.save(cookieHeader: "auth=live")
        let poller = makePoller(
            console: { _, _ in throw URLError(.timedOut) },
            local: { (OpenCodeSnapshot(windows: [], models: []), nil) }
        )
        await poller.refreshNow()
        XCTAssertEqual(poller.dataSourceLabel, "Local estimate")
        XCTAssertEqual(poller.lastError, "Console fetch failed — showing local estimate.")
    }

    func testSignedOutEstimateAsksToSignIn() async {
        let poller = makePoller(console: { _, _ in XCTFail("no console without a cookie"); throw CancellationError() },
                                local: { (OpenCodeSnapshot(windows: [], models: []), nil) })
        await poller.refreshNow()
        XCTAssertEqual(poller.lastError, "Showing local estimate. Sign in to OpenCode for official Go usage.")
    }

    func testMissingLocalSnapshotKeepsTheHourlyChart() async {
        let hourly = OpenCodeDayHourlyUsage(
            dayStart: Calendar.current.startOfDay(for: Date()),
            hours: (0..<24).map { OpenCodeHourUsage(hour: $0, segments: []) },
            legend: []
        )
        let poller = makePoller(console: { _, _ in throw CancellationError() }, local: { (nil, hourly) })
        await poller.refreshNow()
        XCTAssertEqual(poller.dayHourlyUsage, hourly)
        XCTAssertNil(poller.snapshot)
        XCTAssertEqual(poller.lastError, "Could not read local OpenCode usage.")
    }

    func testLocalReadErrorIsReported() async {
        let poller = makePoller(console: { _, _ in throw CancellationError() },
                                local: { throw OpenCodeLocalStatsError.openFailed("locked") })
        await poller.refreshNow()
        XCTAssertEqual(poller.lastError, OpenCodeLocalStatsError.openFailed("locked").localizedDescription)
    }

    /// The default local fetch reads the injected database; a missing one
    /// gives no snapshot and no hourly chart.
    func testDefaultLocalFetchReadsTheInjectedDatabase() async {
        let poller = makePoller(console: { _, _ in throw CancellationError() })
        await poller.refreshNow()
        XCTAssertNil(poller.snapshot)
        XCTAssertNil(poller.dayHourlyUsage)
        XCTAssertEqual(poller.lastError, "Could not read local OpenCode usage.")
    }

    func testIntervalAccountAndClearing() async throws {
        auth.save(cookieHeader: "auth=live")
        auth.saveAccountEmail("me@example.com")
        let poller = makePoller(console: { _, _ in (self.consoleSnapshot(), "wrk") }, local: { (nil, nil) })
        poller.start()
        defer { poller.stop() }
        await poller.refreshNow()
        XCTAssertEqual(poller.alertAccount, "me@example.com")
        let idle = try XCTUnwrap(poller.pollingLoop.delayUntilDue())
        poller.menuIsOpen = true
        XCTAssertLessThan(try XCTUnwrap(poller.pollingLoop.delayUntilDue()), idle)
        auth.accountReset.send()
        XCTAssertNil(poller.snapshot)
        XCTAssertNil(poller.dailyBudgetDays(weekOffset: -1))
        settings.enabledProviderIDs = [.claude]
        XCTAssertNil(poller.pollingLoop.delayUntilDue())
    }
}
