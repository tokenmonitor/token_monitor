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

    private func makePoller(
        daily: DailyQuotaDeltaStore,
        auth: ChatGPTAuthSession? = nil,
        response: ChatGPTUsageResponse
    ) throws -> ChatGPTUsagePoller {
        let suite = "ChatGPTPoller-\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suite))
        addTeardownBlock { defaults.removePersistentDomain(forName: suite) }
        let settings = AppSettings(defaults: defaults)
        settings.selectedProvider = .chatgpt
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
                ChatGPTUsageClient.Fetch(response: response, fetchedAt: Date(), setCookieHeaders: [])
            }
        )
    }
}
