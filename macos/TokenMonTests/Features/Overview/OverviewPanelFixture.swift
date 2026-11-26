import Foundation
@testable import TokenMon

/// Every provider's poller and session, with stubbed fetches that return a
/// distinct usage per provider (Grok 42%, OpenCode 50%, Cursor 61%, Claude
/// 73%, ChatGPT 27%, OpenRouter 25%, Grokbot 38%).
@MainActor
final class OverviewPanelFixture {
    let settings: AppSettings
    let grokAuth: GrokAuthSession
    let openCodeAuth: OpenCodeAuthSession
    let cursorAuth: CursorAuthSession
    let claudeAuth: ClaudeAuthSession
    let chatGPTAuth: ChatGPTAuthSession
    let openRouterAuth: OpenRouterAuthSession
    let grokHourly: HourlyDeltaActivityStore
    let claudeHourly: HourlyDeltaActivityStore
    let grokbotHourly: HourlyDeltaActivityStore
    let history: OverviewHourlyHistoryStore
    private(set) var grokPoller: GrokUsagePoller!
    private(set) var openCodePoller: OpenCodeUsagePoller!
    private(set) var cursorPoller: CursorUsagePoller!
    private(set) var claudePoller: ClaudeUsagePoller!
    private(set) var chatGPTPoller: ChatGPTUsagePoller!
    private(set) var openRouterPoller: OpenRouterUsagePoller!
    private(set) var grokbotPoller: GrokbotUsagePoller!
    /// What the OpenRouter fetch returns; tests swap it before refreshing.
    var openRouterSnapshot = OverviewPanelFixture.openRouter(budgetUSD: 100, usedUSD: 25, remainingUSD: 75)
    /// Hours the Cursor and OpenCode fetches report for today.
    var cursorHourly = CursorDayHourlyUsage(dayStart: Date(), hourWeights: [], quotaHourWeights: [])
    var openCodeHourly: OpenCodeDayHourlyUsage?
    /// The OpenCode plan's monthly limit; spend is always half of it.
    var openCodeLimitUSD: Double = 60
    /// False when the local read finds hours but no usage snapshot.
    var openCodeHasLocalSnapshot = true

    /// Today's chart inputs read from this fixture's sessions and stores.
    var sources: OverviewTodaySources {
        OverviewTodaySources(
            settings: settings,
            grokAuth: grokAuth,
            claudeAuth: claudeAuth,
            cursorAuth: cursorAuth,
            grokHourly: grokHourly,
            claudeHourly: claudeHourly,
            grokbotHourly: grokbotHourly,
            cursorPoller: cursorPoller,
            openCodePoller: openCodePoller
        )
    }

    init(directory: URL, defaults: UserDefaults) {
        settings = AppSettings(defaults: defaults)
        settings.selectedProvider = .overview
        grokAuth = GrokAuthSession(directory: directory, cookieJars: .isolated())
        openCodeAuth = OpenCodeAuthSession(directory: directory, cookieJars: .isolated())
        cursorAuth = CursorAuthSession(directory: directory, cookieJars: .isolated())
        claudeAuth = ClaudeAuthSession(directory: directory, cookieJars: .isolated())
        chatGPTAuth = ChatGPTAuthSession(directory: directory, cookieJars: .isolated())
        openRouterAuth = OpenRouterAuthSession(directory: directory)
        let files = FileBackedStringStore(directory: directory, filenamePrefix: "activity_")
        grokHourly = HourlyDeltaActivityStore(store: files, storageKey: "grok")
        claudeHourly = HourlyDeltaActivityStore(store: files, storageKey: "claude")
        grokbotHourly = HourlyDeltaActivityStore(store: files, storageKey: "grokbot")
        history = OverviewHourlyHistoryStore(store: files)
        makePollers(directory: directory)
    }

    private func makePollers(directory: URL) {
        let files = FileBackedStringStore(directory: directory, filenamePrefix: "daily_")
        grokPoller = GrokUsagePoller(
            auth: grokAuth,
            history: GrokHistoryStore(inMemory: true),
            settings: settings,
            grokHourly: grokHourly,
            fetchUsage: { _, _ in
                WeeklyUsageSnapshot(usedPercent: 42, remainingPercent: 58, resetsAt: Date().addingTimeInterval(3600))
            },
            fetchSavedResets: { _ in nil }
        )
        openCodePoller = OpenCodeUsagePoller(
            settings: settings,
            auth: openCodeAuth,
            fetchConsole: { [weak self] _, _ in
                guard let self else { throw CancellationError() }
                return (Self.openCode(isEstimated: false, limitUSD: openCodeLimitUSD), "wrk_test")
            },
            fetchLocal: { [weak self] in
                guard let self else { throw CancellationError() }
                let snapshot = openCodeHasLocalSnapshot ? Self.openCode(isEstimated: true, limitUSD: openCodeLimitUSD) : nil
                return (snapshot, openCodeHourly)
            }
        )
        cursorPoller = CursorUsagePoller(
            settings: settings,
            auth: cursorAuth,
            daily: DailyQuotaDeltaStore(store: files, storageKey: "cursor"),
            fetchSnapshot: { [weak self] _ in
                guard let self else { throw CancellationError() }
                return (Self.cursor(), cursorHourly, [:])
            }
        )
        claudePoller = ClaudeUsagePoller(
            settings: settings,
            auth: claudeAuth,
            hourly: claudeHourly,
            daily: DailyQuotaDeltaStore(store: files, storageKey: "claude"),
            fetchUsage: { _ in
                let week = ClaudeUsageWindow(usedPercent: 73, resetsAt: Date().addingTimeInterval(86400))
                return (ClaudeUsageResponse(fiveHour: nil, sevenDay: week), Date())
            },
            loadCodeUsage: { _ in nil }
        )
        chatGPTPoller = ChatGPTUsagePoller(
            settings: settings,
            auth: chatGPTAuth,
            daily: DailyQuotaDeltaStore(store: files, storageKey: "chatgpt"),
            fetchUsage: { _, _ in
                let response = ChatGPTUsageResponse(
                    planName: "Plus",
                    allowed: true,
                    limitReached: false,
                    primary: ChatGPTUsageWindow(usedPercent: 27, resetsAt: nil, windowSeconds: nil),
                    secondary: nil
                )
                return ChatGPTUsageClient.Fetch(response: response, fetchedAt: Date(), setCookieHeaders: [])
            }
        )
        openRouterPoller = OpenRouterUsagePoller(
            settings: settings,
            auth: openRouterAuth,
            fetchSnapshot: { [weak self] _ in
                guard let self else { throw CancellationError() }
                return openRouterSnapshot
            }
        )
        grokbotPoller = GrokbotUsagePoller(
            settings: settings,
            auth: cursorAuth,
            hourly: grokbotHourly,
            daily: DailyQuotaDeltaStore(store: files, storageKey: "grokbot"),
            fetchSnapshot: { _, _ in
                GrokbotSnapshot(fetchedAt: Date(), usedPercent: 38, resetsAt: Date().addingTimeInterval(3 * 86400))
            },
            fetchBreakdown: { _ in nil }
        )
    }

    func signInAll() {
        grokAuth.save(cookieHeader: "sso=live")
        openCodeAuth.save(cookieHeader: "auth=live")
        cursorAuth.save(cookieHeader: "WorkosCursorSessionToken=live")
        claudeAuth.save(cookieHeader: "sessionKey=live")
        chatGPTAuth.save(cookieHeader: "__Secure-next-auth.session-token=live")
        openRouterAuth.saveAPIKey("sk-or-v1-live")
    }

    func refreshAll() async {
        await grokPoller.refreshNow()
        await openCodePoller.refreshNow()
        await cursorPoller.refreshNow()
        await claudePoller.refreshNow()
        await chatGPTPoller.refreshNow()
        await openRouterPoller.refreshNow()
        await grokbotPoller.refreshNow()
    }

    func panel(
        todaySeries: @escaping (Date) -> OverviewDaySeries = { _ in OverviewDaySeries() },
        openSignIn: @escaping (MonitorProvider) -> Void = { _ in },
        selectOpenRouter: @escaping () -> Void = {},
        openPreferences: @escaping () -> Void = {}
    ) -> OverviewPanelView {
        OverviewPanelView(
            grokPoller: grokPoller,
            openCodePoller: openCodePoller,
            cursorPoller: cursorPoller,
            claudePoller: claudePoller,
            chatGPTPoller: chatGPTPoller,
            openRouterPoller: openRouterPoller,
            grokbotPoller: grokbotPoller,
            settings: settings,
            grokHourly: grokHourly,
            claudeHourly: claudeHourly,
            grokbotHourly: grokbotHourly,
            hourlyHistory: history,
            todaySeries: todaySeries,
            grokAuth: grokAuth,
            openCodeAuth: openCodeAuth,
            cursorAuth: cursorAuth,
            claudeAuth: claudeAuth,
            chatGPTAuth: chatGPTAuth,
            openRouterAuth: openRouterAuth,
            openSignIn: openSignIn,
            selectOpenRouter: selectOpenRouter,
            openPreferences: openPreferences
        )
    }

    // MARK: - Stub data

    nonisolated static func openCode(isEstimated: Bool, limitUSD: Double) -> OpenCodeSnapshot {
        OpenCodeSnapshot(
            windows: [
                OpenCodeWindowUsage(kind: .monthly, usedUSD: limitUSD / 2, limitUSD: limitUSD, resetsAt: nil, sessionCount: 1)
            ],
            models: [],
            isEstimated: isEstimated
        )
    }

    nonisolated static func cursor() -> CursorSnapshot {
        CursorSnapshot(
            fetchedAt: Date(),
            usedPercent: 61,
            pools: [],
            billingCycleStart: nil,
            billingCycleEnd: Date().addingTimeInterval(10 * 86400),
            membershipType: nil,
            planUsedUSD: nil,
            planLimitUSD: nil,
            onDemandEnabled: false,
            onDemandUsedUSD: nil,
            onDemandLimitUSD: nil,
            costStats: nil,
            accountEmail: nil
        )
    }

    nonisolated static func openRouter(budgetUSD: Double?, usedUSD: Double, remainingUSD: Double?) -> OpenRouterSnapshot {
        OpenRouterSnapshot(
            fetchedAt: Date(),
            keyLabel: nil,
            isManagementKey: false,
            isFreeTier: false,
            accountCreditsUSD: nil,
            accountUsedUSD: nil,
            keyUsageUSD: usedUSD,
            keyUsageDailyUSD: 0,
            keyUsageWeeklyUSD: 0,
            keyUsageMonthlyUSD: 0,
            keyLimitUSD: nil,
            keyLimitRemainingUSD: nil,
            budgetSource: budgetUSD == nil ? nil : .accountCredits,
            budgetUSD: budgetUSD,
            usedUSD: usedUSD,
            remainingUSD: remainingUSD
        )
    }
}
