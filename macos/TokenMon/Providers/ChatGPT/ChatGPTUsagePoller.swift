import Combine
import Foundation
import Observation
import os

/// Polls ChatGPT usage for the captured session cookie and builds the
/// weekly window's daily bars.
@MainActor
@Observable
final class ChatGPTUsagePoller: ProviderUsagePoller {
    private(set) var snapshot: ChatGPTSnapshot?
    private(set) var isRefreshing = false
    private(set) var lastError: String?
    private(set) var lastRefreshedAt: Date?
    /// Daily bars for the current weekly window, anchored to its `reset_at`:
    /// % of the weekly pool used per calendar day.
    private(set) var dailyBudgetDays: [DailyBudgetDay]?
    var menuIsOpen = false {
        didSet { if menuIsOpen != oldValue { pollingLoop.wake() } }
    }

    private let settings: AppSettings
    private let auth: ChatGPTAuthSession
    private let daily: DailyQuotaDeltaStore
    /// Injected fetch seam (tests supply a fake); defaults to the live client.
    /// Takes the cookie header and the session generation.
    private let fetchUsage: (String, Int) async throws -> ChatGPTUsageClient.Fetch
    /// Access token reused across polls by the live client.
    private let tokenCache: ChatGPTAccessTokenCache
    private let logger = Logger(category: "ChatGPT")
    @ObservationIgnored private var cancellables = Set<AnyCancellable>()

    /// Last observed weekly `reset_at`; anchors the bars when a later payload
    /// omits it.
    private var weeklyResetsAt: Date?
    /// Instant the current bars were built against, so earlier weeks shift from
    /// the same window.
    private var budgetReferenceNow: Date?

    @ObservationIgnored private(set) lazy var pollingLoop = PollingLoop(
        interval: { [weak self] in self?.pollInterval() },
        refresh: { [weak self] in await self?.performRefresh() ?? .skipped }
    )

    init(
        settings: AppSettings,
        auth: ChatGPTAuthSession,
        daily: DailyQuotaDeltaStore,
        fetchUsage: ((String, Int) async throws -> ChatGPTUsageClient.Fetch)? = nil
    ) {
        self.settings = settings
        self.auth = auth
        self.daily = daily
        let tokenCache = ChatGPTAccessTokenCache()
        self.tokenCache = tokenCache
        self.fetchUsage = fetchUsage ?? { cookieHeader, generation in
            try await ChatGPTUsageClient(
                cookieHeader: cookieHeader,
                tokenCache: tokenCache,
                tokenCacheKey: String(generation)
            ).fetchUsage()
        }
        // Signing out drops the snapshot. The persisted daily history is wiped
        // only when a different account signs in.
        auth.signedOut
            .sink { [weak self] in self?.clearSnapshot() }
            .store(in: &cancellables)
        auth.accountReset
            .sink { [weak self] in
                self?.clearSnapshot()
                self?.daily.clear()
            }
            .store(in: &cancellables)
    }

    var headlineUsedPercent: Double? { snapshot?.headlineUsedPercent }
    var headlineResetsAt: Date? { snapshot?.resetsAt }
    var alertAccount: String? { auth.accountEmail }

    func clearSnapshot() {
        tokenCache.clear()
        snapshot = nil
        dailyBudgetDays = nil
        weeklyResetsAt = nil
        budgetReferenceNow = nil
        lastError = nil
        lastRefreshedAt = nil
    }

    private func performRefresh() async -> PollOutcome {
        guard settings.isProviderEnabled(.chatgpt) else { return .skipped }
        isRefreshing = true
        defer { isRefreshing = false }

        guard let cookieHeader = auth.cookieHeader(), !cookieHeader.isEmpty else {
            auth.needsSignIn = true
            if snapshot == nil {
                lastError = "Sign in to ChatGPT to load usage."
            }
            return .skipped
        }

        let generation = auth.sessionGeneration
        return await ProviderRefresh.run(
            auth: auth,
            logger: logger,
            label: "ChatGPT",
            fetch: { try await fetchUsage(cookieHeader, generation) },
            publish: { fetch in
                publish(fetch)
                return .success
            },
            onRejected: { tokenCache.clear() },
            reportError: { lastError = $0 }
        )
    }

    private func publish(_ fetch: ChatGPTUsageClient.Fetch) {
        // Fold a renewed session cookie back into the store before it hard-expires.
        auth.applyRefreshedCookies(fetch.setCookieHeaders)
        let response = fetch.response
        let fresh = ChatGPTSnapshot(
            fetchedAt: fetch.fetchedAt,
            planName: response.planName,
            allowed: response.allowed,
            limitReached: response.limitReached,
            primary: response.primary,
            secondary: response.secondary,
            savedResets: response.savedResets
        )
        snapshot = fresh
        lastError = nil
        lastRefreshedAt = Date()
        auth.needsSignIn = false
        recordWeeklyUsage(fresh)
        let headline = response.primary?.usedPercent ?? response.secondary?.usedPercent ?? 0
        logger.info("ChatGPT refresh: \(Int(headline.rounded()))% used")
    }

    private func pollInterval() -> TimeInterval? {
        settings.pollInterval(for: .chatgpt, menuIsOpen: menuIsOpen)
    }

    /// Records the weekly window's used % in the daily store and rebuilds the
    /// bars. A moved `reset_at` together with a used-% drop is a rollover or
    /// early reset, which the store recognizes and keeps earlier days for.
    private func recordWeeklyUsage(_ fresh: ChatGPTSnapshot) {
        if let week = fresh.weeklyWindow {
            if let resetsAt = week.resetsAt {
                weeklyResetsAt = resetsAt
            }
            daily.record(
                windowUsedPercent: week.usedPercent,
                at: fresh.fetchedAt,
                window: QuotaWindow(start: nil, resetsAt: week.resetsAt)
            )
        }
        budgetReferenceNow = fresh.fetchedAt
        dailyBudgetDays = dailyBudgetDays(weekOffset: 0)
    }

    /// Bars for the weekly window `weekOffset` steps before the current one
    /// (`0` is the current one). Reads the live daily store, so the panel can
    /// browse earlier weeks without a new poll. Empty until a weekly reset
    /// instant has been observed.
    func dailyBudgetDays(weekOffset: Int) -> [DailyBudgetDay] {
        let now = budgetReferenceNow ?? Date()
        guard let resetsAt = weeklyResetsAt(now: now) else { return [] }
        return DailyBudget.buildWeeklyWindowDays(
            limitUSD: 100,
            daysInPeriod: 7,
            resetsAt: resetsAt,
            spentByDay: daily.spentByDay,
            windowStart: daily.windowStart,
            interruptedWindowStart: daily.interruptedWindowStart,
            weekOffset: weekOffset,
            now: now
        )
    }

    /// Weekly reset instant the bars anchor to: the latest payload's, else the
    /// last one seen this run, else the one the daily store persisted, carried
    /// forward by whole weeks once it has passed.
    func weeklyResetsAt(now: Date = Date()) -> Date? {
        if let live = snapshot?.weeklyWindow?.resetsAt {
            return live
        }
        return DailyBudget.projectWeeklyReset(weeklyResetsAt ?? daily.windowResetsAt, now: now)
    }
}
