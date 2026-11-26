@testable import TokenMon
import XCTest

@MainActor
final class ChatGPTUsagePollerTests: XCTestCase {
    private var dir: URL!

    override func setUpWithError() throws {
        try super.setUpWithError()
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: dir)
        super.tearDown()
    }

    // MARK: - Weekly window

    func testWeeklyWindowPrefersTheSevenDayWindow() {
        let week = ChatGPTUsageWindow(usedPercent: 20, windowSeconds: ChatGPTSnapshot.weekSeconds)
        let fiveHour = ChatGPTUsageWindow(usedPercent: 5, windowSeconds: 18_000)
        XCTAssertEqual(snapshot(primary: fiveHour, secondary: week).weeklyWindow, week)
        // A free plan reports the weekly window as the primary one.
        XCTAssertEqual(snapshot(primary: week, secondary: nil).weeklyWindow, week)
    }

    func testWeeklyWindowFallsBackToSecondaryOnlyWithoutLength() {
        let unlabeled = ChatGPTUsageWindow(usedPercent: 30)
        XCTAssertEqual(snapshot(primary: nil, secondary: unlabeled).weeklyWindow, unlabeled)
        let daily = ChatGPTUsageWindow(usedPercent: 30, windowSeconds: 86_400)
        XCTAssertNil(snapshot(primary: nil, secondary: daily).weeklyWindow)
        XCTAssertNil(snapshot(primary: ChatGPTUsageWindow(usedPercent: 1), secondary: nil).weeklyWindow)
    }

    // MARK: - Daily bars

    func testRefreshRecordsWeeklyUsageAndPublishesSevenBars() async throws {
        let store = dailyStore()
        let resetsAt = Date().addingTimeInterval(3 * 86_400)
        let poller = try makePoller(daily: store, response: weeklyResponse(percent: 42, resetsAt: resetsAt))

        await poller.refreshNow()

        XCTAssertEqual(poller.dailyBudgetDays?.count, 7)
        XCTAssertEqual(store.lastUsedPercent ?? -1, 42, accuracy: 0.001)
        XCTAssertEqual(store.windowResetsAt, resetsAt)
        XCTAssertEqual(poller.weeklyResetsAt(), resetsAt)
        XCTAssertEqual(poller.dailyBudgetDays(weekOffset: 0).count, 7)
    }

    func testRefreshWithoutWeeklyWindowShowsNoBars() async throws {
        let store = dailyStore()
        let response = ChatGPTUsageResponse(
            planName: "plus",
            allowed: true,
            limitReached: false,
            primary: ChatGPTUsageWindow(usedPercent: 10, resetsAt: Date().addingTimeInterval(3_600), windowSeconds: 18_000),
            secondary: nil
        )
        let poller = try makePoller(daily: store, response: response)

        await poller.refreshNow()

        XCTAssertEqual(poller.dailyBudgetDays, [])
        XCTAssertNil(store.lastUsedPercent)
    }

    /// Signing out keeps the daily history for the same account; a different
    /// account signing in clears it.
    func testAccountSwitchClearsDailyHistoryButSignOutKeepsIt() async throws {
        let store = dailyStore()
        let auth = ChatGPTAuthSession(directory: dir)
        auth.save(cookieHeader: "__Secure-next-auth.session-token=a")
        auth.saveAccountEmail("first@example.com")
        let poller = try makePoller(
            daily: store,
            auth: auth,
            response: weeklyResponse(percent: 12, resetsAt: Date().addingTimeInterval(2 * 86_400))
        )
        await poller.refreshNow()
        XCTAssertNotNil(store.lastUsedPercent)

        auth.signOut()
        XCTAssertNil(poller.dailyBudgetDays)
        XCTAssertNotNil(store.lastUsedPercent)

        auth.saveAccountEmail("second@example.com")
        XCTAssertNil(store.lastUsedPercent)
        XCTAssertTrue(store.spentByDay.isEmpty)
    }

    // MARK: - Gating and lifecycle

    func testSignedOutRefreshAsksForSignInWithoutFetching() async throws {
        var fetches = 0
        let auth = ChatGPTAuthSession(directory: dir)
        let poller = try makePoller(daily: dailyStore(), auth: auth, response: weeklyResponse(percent: 1, resetsAt: Date())) {
            fetches += 1
        }
        await poller.refreshNow()
        XCTAssertEqual(fetches, 0)
        XCTAssertTrue(auth.needsSignIn)
        XCTAssertEqual(poller.lastError, "Sign in to ChatGPT to load usage.")
        XCTAssertNil(poller.headlineUsedPercent)
    }

    func testDisabledProviderSkipsAndParksItsLoop() async throws {
        var fetches = 0
        let poller = try makePoller(daily: dailyStore(), response: weeklyResponse(percent: 1, resetsAt: Date())) {
            fetches += 1
        }
        pollerSettings.enabledProviderIDs = [.cursor]
        await poller.refreshNow()
        XCTAssertEqual(fetches, 0)
        XCTAssertNil(poller.pollingLoop.delayUntilDue())
    }

    func testPollIntervalFollowsTheMenuState() async throws {
        let poller = try makePoller(daily: dailyStore(), response: weeklyResponse(percent: 1, resetsAt: Date()))
        poller.start()
        defer { poller.stop() }
        await poller.refreshNow()
        let idle = try XCTUnwrap(poller.pollingLoop.delayUntilDue())
        poller.menuIsOpen = true
        let active = try XCTUnwrap(poller.pollingLoop.delayUntilDue())
        XCTAssertLessThan(active, idle, "an open menu polls faster")
        XCTAssertLessThanOrEqual(active, pollerSettings.pollSeconds(menuIsOpen: true) * 1.1)
    }

    func testHeadlineAndAlertAccountReadTheSnapshotAndSession() async throws {
        let resetsAt = Date().addingTimeInterval(3_600)
        let auth = ChatGPTAuthSession(directory: dir)
        auth.save(cookieHeader: "__Secure-next-auth.session-token=a")
        auth.saveAccountEmail("me@example.com")
        let poller = try makePoller(daily: dailyStore(), auth: auth, response: weeklyResponse(percent: 42, resetsAt: resetsAt))
        await poller.refreshNow()
        XCTAssertEqual(poller.headlineUsedPercent ?? -1, 5, accuracy: 0.001, "the 5-hour primary window leads")
        XCTAssertNotNil(poller.headlineResetsAt)
        XCTAssertEqual(poller.alertAccount, "me@example.com")
        poller.clearSnapshot()
        XCTAssertNil(poller.headlineUsedPercent)
        XCTAssertNil(poller.lastRefreshedAt)
    }

    func testStoredResetIsProjectedForwardWhenNoPayloadHasOne() async throws {
        let store = dailyStore()
        let past = Date().addingTimeInterval(-86_400)
        let first = try makePoller(daily: store, response: weeklyResponse(percent: 10, resetsAt: past))
        await first.refreshNow()
        let fresh = try makePoller(daily: store, response: weeklyResponse(percent: 10, resetsAt: past))
        let projected = try XCTUnwrap(fresh.weeklyResetsAt())
        XCTAssertGreaterThan(projected, Date(), "a passed reset is carried forward by whole weeks")
        XCTAssertEqual(projected.timeIntervalSince(past).truncatingRemainder(dividingBy: 7 * 86_400), 0, accuracy: 1)
        XCTAssertEqual(fresh.dailyBudgetDays(weekOffset: -1).count, 7)
    }

    // MARK: - Helpers

    private func snapshot(primary: ChatGPTUsageWindow?, secondary: ChatGPTUsageWindow?) -> ChatGPTSnapshot {
        ChatGPTSnapshot(fetchedAt: Date(), allowed: true, limitReached: false, primary: primary, secondary: secondary)
    }

    private func weeklyResponse(percent: Double, resetsAt: Date) -> ChatGPTUsageResponse {
        ChatGPTUsageResponse(
            planName: "plus",
            allowed: true,
            limitReached: false,
            primary: ChatGPTUsageWindow(usedPercent: 5, resetsAt: Date().addingTimeInterval(3_600), windowSeconds: 18_000),
            secondary: ChatGPTUsageWindow(usedPercent: percent, resetsAt: resetsAt, windowSeconds: ChatGPTSnapshot.weekSeconds)
        )
    }

    private func dailyStore() -> DailyQuotaDeltaStore {
        DailyQuotaDeltaStore(
            store: FileBackedStringStore(directory: dir, filenamePrefix: "daily_"),
            storageKey: "chatgpt_daily_usage"
        )
    }

    /// Settings of the poller built last.
    private var pollerSettings: AppSettings!

    private func makePoller(
        daily: DailyQuotaDeltaStore,
        auth: ChatGPTAuthSession? = nil,
        response: ChatGPTUsageResponse,
        onFetch: @escaping () -> Void = {}
    ) throws -> ChatGPTUsagePoller {
        let suite = "ChatGPTPoller-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        addTeardownBlock { defaults.removePersistentDomain(forName: suite) }
        let settings = AppSettings(defaults: defaults)
        settings.selectedProvider = .chatgpt
        pollerSettings = settings
        let session = auth ?? {
            let fresh = ChatGPTAuthSession(directory: dir)
            fresh.save(cookieHeader: "__Secure-next-auth.session-token=test")
            return fresh
        }()
        return ChatGPTUsagePoller(
            settings: settings,
            auth: session,
            daily: daily,
            fetchUsage: { _, _ in
                onFetch()
                return ChatGPTUsageClient.Fetch(response: response, fetchedAt: Date(), setCookieHeaders: [])
            }
        )
    }
}
