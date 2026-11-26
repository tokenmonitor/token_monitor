@testable import TokenMon
import XCTest

/// The Cursor poller's refresh: sign-in gating, what one poll publishes, the
/// carry-forward of event figures when the events fetch fails, account
/// switches, and the poll interval.
@MainActor
final class CursorPollerRefreshTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var settings: AppSettings!
    private var auth: CursorAuthSession!
    private var daily: DailyQuotaDeltaStore!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "CursorPollerRefresh-\(UUID().uuidString)"
        settings = AppSettings(defaults: try XCTUnwrap(UserDefaults(suiteName: suiteName)))
        settings.selectedProvider = .cursor
        auth = CursorAuthSession(directory: dir)
        daily = DailyQuotaDeltaStore(
            store: FileBackedStringStore(directory: dir, filenamePrefix: "daily_"),
            storageKey: "cursor_daily_usage"
        )
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        UserDefaults(suiteName: suiteName)?.removePersistentDomain(forName: suiteName)
    }

    private typealias Fetched = (CursorSnapshot, CursorDayHourlyUsage, [Date: Double])

    private func snapshot(percent: Double, email: String? = nil, cost: Double? = nil) throws -> CursorSnapshot {
        let start = ISO8601DateFormatter().string(from: Date().addingTimeInterval(-5 * 86_400))
        let end = ISO8601DateFormatter().string(from: Date().addingTimeInterval(25 * 86_400))
        var snap = try CursorUsageClient.parseSummary(data: Data("""
        {"billingCycleStart":"\(start)","billingCycleEnd":"\(end)","membershipType":"pro",
         "individualUsage":{"plan":{"used":100,"limit":2000,"totalPercentUsed":\(percent)}}}
        """.utf8))
        snap.accountEmail = email
        snap.costStats = cost.map { CursorCostStats(meteredCycleUSD: $0, cycleTokens: 10) }
        return snap
    }

    private func hourly(_ weight: Double) -> CursorDayHourlyUsage {
        var hours = Array(repeating: 0.0, count: 24)
        hours[9] = weight
        return CursorDayHourlyUsage(dayStart: Calendar.current.startOfDay(for: Date()), hourWeights: hours, quotaHourWeights: hours)
    }

    private func makePoller(_ fetch: @escaping (String) async throws -> Fetched) -> CursorUsagePoller {
        CursorUsagePoller(settings: settings, auth: auth, daily: daily, fetchSnapshot: fetch)
    }

    func testSignedOutRefreshAsksForSignInWithoutFetching() async {
        var fetches = 0
        let poller = makePoller { _ in
            fetches += 1
            throw CancellationError()
        }
        await poller.refreshNow()
        XCTAssertEqual(fetches, 0)
        XCTAssertTrue(auth.needsSignIn)
        XCTAssertEqual(poller.lastError, "Sign in to Cursor to load usage.")
    }

    func testRefreshPublishesSnapshotBarsAndTheAccountEmail() async throws {
        auth.save(cookieHeader: "WorkosCursorSessionToken=t")
        let snap = try snapshot(percent: 12, email: "me@example.com", cost: 4)
        let poller = makePoller { _ in (snap, self.hourly(2), [:]) }

        await poller.refreshNow()

        XCTAssertEqual(poller.snapshot?.usedPercent ?? -1, 12, accuracy: 0.001)
        XCTAssertEqual(poller.headlineUsedPercent ?? -1, 12, accuracy: 0.001)
        XCTAssertEqual(poller.headlineResetsAt, snap.billingCycleEnd)
        XCTAssertEqual(auth.accountEmail, "me@example.com")
        XCTAssertEqual(poller.alertAccount, "me@example.com")
        XCTAssertEqual(poller.dataSourceLabel, "Cursor dashboard")
        XCTAssertEqual(poller.dayHourlyUsage?.hourWeights[9], 2)
        XCTAssertEqual(poller.dailyBudgetDays?.count, 7)
        XCTAssertEqual(poller.dailyBudgetDays(weekOffset: -1)?.count, 7)
        XCTAssertFalse(auth.needsSignIn)
    }

    /// A poll whose events fetch failed (no cost stats) keeps the previous
    /// poll's event figures for the same billing cycle.
    func testFailedEventsFetchKeepsThePreviousEventFigures() async throws {
        auth.save(cookieHeader: "WorkosCursorSessionToken=t")
        let first = try snapshot(percent: 10, cost: 4)
        var second = first
        second.usedPercent = 11
        second.costStats = nil
        var replies: [Fetched] = [(first, hourly(3), [:]), (second, hourly(0), [:])]
        let poller = makePoller { _ in replies.removeFirst() }

        await poller.refreshNow()
        await poller.refreshNow()

        XCTAssertEqual(poller.snapshot?.usedPercent ?? -1, 11, accuracy: 0.001)
        XCTAssertEqual(poller.snapshot?.costStats?.meteredCycleUSD ?? -1, 4, accuracy: 0.001)
        XCTAssertEqual(poller.dayHourlyUsage?.hourWeights[9], 3, "today's hourly figures carry over")
    }

    func testAccountSwitchClearsSnapshotAndDailyHistory() async throws {
        auth.save(cookieHeader: "WorkosCursorSessionToken=t")
        let snap = try snapshot(percent: 20)
        let poller = makePoller { _ in (snap, self.hourly(1), [:]) }
        await poller.refreshNow()
        XCTAssertNotNil(daily.lastUsedPercent)

        auth.accountReset.send()

        XCTAssertNil(poller.snapshot)
        XCTAssertNil(poller.dailyBudgetDays(weekOffset: 0), "no bars before the next refresh")
        XCTAssertNil(daily.lastUsedPercent)
    }

    func testPollIntervalFollowsMenuAndEnablement() async throws {
        auth.save(cookieHeader: "WorkosCursorSessionToken=t")
        let snap = try snapshot(percent: 1)
        var fetches = 0
        let poller = makePoller { _ in
            fetches += 1
            return (snap, self.hourly(0), [:])
        }
        poller.start()
        defer { poller.stop() }
        await poller.refreshNow()
        let idle = try XCTUnwrap(poller.pollingLoop.delayUntilDue())
        poller.menuIsOpen = true
        XCTAssertLessThan(try XCTUnwrap(poller.pollingLoop.delayUntilDue()), idle)
        settings.enabledProviderIDs = [.claude]
        XCTAssertNil(poller.pollingLoop.delayUntilDue())
        let before = fetches
        await poller.refreshNow()
        XCTAssertEqual(fetches, before, "a disabled provider is not refreshed")
    }
}
