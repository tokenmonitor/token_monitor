@testable import TokenMon
import XCTest

/// The Grokbot poller's refresh: the shared Cursor session gate, what one
/// poll publishes, the best-effort usage breakdown, account switches, and
/// the poll interval.
@MainActor
final class GrokbotPollerRefreshTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var settings: AppSettings!
    private var auth: CursorAuthSession!
    private var hourly: HourlyDeltaActivityStore!
    private var daily: DailyQuotaDeltaStore!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "GrokbotPollerRefresh-\(UUID().uuidString)"
        settings = AppSettings(defaults: try XCTUnwrap(UserDefaults(suiteName: suiteName)))
        settings.selectedProvider = .grokbot
        auth = CursorAuthSession(directory: dir)
        hourly = HourlyDeltaActivityStore(
            store: FileBackedStringStore(directory: dir, filenamePrefix: "hourly_"),
            storageKey: "grokbot_hourly"
        )
        daily = DailyQuotaDeltaStore(
            store: FileBackedStringStore(directory: dir, filenamePrefix: "daily_"),
            storageKey: "grokbot_daily"
        )
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        UserDefaults(suiteName: suiteName)?.removePersistentDomain(forName: suiteName)
    }

    private func snapshot(_ percent: Double) -> GrokbotSnapshot {
        GrokbotSnapshot(
            fetchedAt: Date(),
            usedPercent: percent,
            periodStart: Date().addingTimeInterval(-2 * 86_400),
            resetsAt: Date().addingTimeInterval(5 * 86_400)
        )
    }

    private let breakdown = GrokbotUsageBreakdown(
        unit: .weeklyLimitPercent,
        periodStart: nil,
        periodEnd: nil,
        slices: [.init(source: .user, value: 12)]
    )

    private func makePoller(
        snapshot: @escaping (String, String?) async throws -> GrokbotSnapshot,
        breakdown: ((String) async throws -> GrokbotUsageBreakdown?)? = nil
    ) -> GrokbotUsagePoller {
        GrokbotUsagePoller(
            settings: settings,
            auth: auth,
            hourly: hourly,
            daily: daily,
            fetchSnapshot: snapshot,
            fetchBreakdown: breakdown
        )
    }

    func testWithoutTheCursorSessionAsksToSignIn() async {
        let poller = makePoller { _, _ in
            XCTFail("no fetch without a session")
            throw CancellationError()
        }
        await poller.refreshNow()
        XCTAssertEqual(poller.lastError, "Sign in to Cursor to load your Grokbot allowance.")
    }

    func testRefreshPublishesSnapshotBarsAndBreakdown() async throws {
        auth.save(cookieHeader: "WorkosCursorSessionToken=t")
        auth.saveAccountEmail("me@example.com")
        var sentEmail: String?
        let poller = makePoller(
            snapshot: { _, email in
                sentEmail = email
                return self.snapshot(30)
            },
            breakdown: { _ in self.breakdown }
        )

        await poller.refreshNow()

        XCTAssertEqual(sentEmail, "me@example.com")
        XCTAssertEqual(poller.headlineUsedPercent ?? -1, 30, accuracy: 0.001)
        XCTAssertNotNil(poller.headlineResetsAt)
        XCTAssertEqual(poller.alertAccount, "me@example.com")
        XCTAssertEqual(poller.breakdown, breakdown)
        XCTAssertEqual(poller.dailyBudgetDays?.count, 7)
        XCTAssertEqual(poller.dailyBudgetDays(weekOffset: -1).count, 7)
    }

    /// A failed breakdown keeps the last one and reports nothing.
    func testBreakdownFailureKeepsTheLastBreakdown() async throws {
        auth.save(cookieHeader: "WorkosCursorSessionToken=t")
        var failBreakdown = false
        let poller = makePoller(
            snapshot: { _, _ in self.snapshot(10) },
            breakdown: { _ in
                if failBreakdown { throw URLError(.timedOut) }
                return self.breakdown
            }
        )
        await poller.refreshNow()
        failBreakdown = true
        await poller.refreshNow()
        XCTAssertEqual(poller.breakdown, breakdown)
        XCTAssertNil(poller.lastError)
    }

    func testAccountSwitchClearsSnapshotAndHistory() async throws {
        auth.save(cookieHeader: "WorkosCursorSessionToken=t")
        let poller = makePoller(snapshot: { _, _ in self.snapshot(40) })
        await poller.refreshNow()
        XCTAssertNotNil(daily.lastUsedPercent)

        auth.accountReset.send()

        XCTAssertNil(poller.snapshot)
        XCTAssertNil(poller.breakdown)
        XCTAssertNil(daily.lastUsedPercent)
        XCTAssertEqual(hourly.hourWeights.reduce(0, +), 0)
        XCTAssertTrue(poller.dailyBudgetDays(weekOffset: 0).isEmpty, "no reset is known after the switch")
    }

    func testPollIntervalFollowsMenuAndEnablement() async throws {
        auth.save(cookieHeader: "WorkosCursorSessionToken=t")
        let poller = makePoller(snapshot: { _, _ in self.snapshot(1) })
        poller.start()
        defer { poller.stop() }
        await poller.refreshNow()
        let idle = try XCTUnwrap(poller.pollingLoop.delayUntilDue())
        poller.menuIsOpen = true
        XCTAssertLessThan(try XCTUnwrap(poller.pollingLoop.delayUntilDue()), idle)
        settings.enabledProviderIDs = [.cursor]
        XCTAssertNil(poller.pollingLoop.delayUntilDue())
    }
}
