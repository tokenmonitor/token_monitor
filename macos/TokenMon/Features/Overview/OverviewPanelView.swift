import AppKit
import SwiftUI

/// Aggregate cards for every enabled provider plus the hourly chart.
struct OverviewPanelView: View {
    let grokPoller: GrokUsagePoller
    let openCodePoller: OpenCodeUsagePoller
    let cursorPoller: CursorUsagePoller
    let claudePoller: ClaudeUsagePoller
    let chatGPTPoller: ChatGPTUsagePoller
    let openRouterPoller: OpenRouterUsagePoller
    let grokbotPoller: GrokbotUsagePoller
    let settings: AppSettings
    // The hourly stores are observed so today's chart redraws when they grow.
    let grokHourly: HourlyDeltaActivityStore
    let claudeHourly: HourlyDeltaActivityStore
    let grokbotHourly: HourlyDeltaActivityStore
    let hourlyHistory: OverviewHourlyHistoryStore
    /// Today's live chart inputs for a moment in today.
    var todaySeries: (Date) -> OverviewDaySeries
    let grokAuth: GrokAuthSession
    let openCodeAuth: OpenCodeAuthSession
    let cursorAuth: CursorAuthSession
    let claudeAuth: ClaudeAuthSession
    let chatGPTAuth: ChatGPTAuthSession
    let openRouterAuth: OpenRouterAuthSession
    /// Opens the sign-in window for a provider's session.
    var openSignIn: (MonitorProvider) -> Void
    var selectOpenRouter: () -> Void
    var openPreferences: () -> Void

    private var appVersion: String {
        let version = Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "1.0.0"
        return "v\(version)"
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .center, spacing: 8) {
                Image(nsImage: ProviderLogo.tokenmon)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFit()
                    .frame(width: 24, height: 24)
                    .foregroundStyle(.primary)
                Text("TokenMon")
                    .font(PanelTypography.title)
                    .foregroundStyle(.primary)
                Spacer()
                Text(appVersion)
                    .font(PanelTypography.bodyDigit)
                    .foregroundStyle(.tertiary)
                SettingsCogButton(action: openPreferences)
            }

            ForEach(settings.visibleUsageProviders) { provider in
                usageCard(for: provider)
            }

            // Re-evaluates at each local midnight so a panel left open across
            // the day boundary switches to the new day. The schedule starts at
            // today's midnight, so the first render shows today.
            TimelineView(.explicit(Self.midnightSchedule(from: Date()))) { context in
                OverviewDailyUsageCard(now: context.date, history: hourlyHistory, todaySeries: todaySeries)
            }
        }
    }

    /// Local midnights from the start of `date`'s day through the next week.
    ///
    /// `TimelineView` renders the latest entry at or before the current time,
    /// so the first entry is today's midnight and the first render shows today.
    static func midnightSchedule(from date: Date, calendar: Calendar = .current) -> [Date] {
        let today = calendar.startOfDay(for: date)
        return (0...7).compactMap { calendar.date(byAdding: .day, value: $0, to: today) }
    }

    @ViewBuilder
    private func usageCard(for provider: MonitorProvider) -> some View {
        switch provider {
        case .grok: grokUsageCard
        case .opencode: openCodeUsageCard
        case .cursor: cursorUsageCard
        case .claude: claudeUsageCard
        case .chatgpt: chatGPTUsageCard
        case .openrouter: openRouterUsageCard
        case .grokbot: grokbotUsageCard
        case .overview: EmptyView()
        }
    }

    private var grokUsageCard: some View {
        let snapshot = grokPoller.snapshot
        let percent = snapshot?.usedPercent ?? 0
        let signedIn = grokAuth.isSignedIn && !grokAuth.needsSignIn
        return usageCard(.grok, period: "Weekly", percent: percent) {
            limitCaption(.grok, hasData: snapshot != nil, resetsAt: snapshot?.resetsAt, percent: percent)
            sessionControls(
                .grok,
                signIn: signedIn ? nil : { openSignIn(.grok) },
                signOut: signedIn ? { grokAuth.signOut() } : nil
            )
        }
    }

    private var openCodeUsageCard: some View {
        let snapshot = openCodePoller.snapshot
        let monthly = snapshot?.windows.first { $0.kind == .monthly }
        let percent = monthly?.usedPercent ?? snapshot?.monthlyUsedPercent ?? 0
        return usageCard(.opencode, period: "Monthly", percent: percent, barColor: ModelPalette.purple.color) {
            limitCaption(.opencode, hasData: snapshot != nil, resetsAt: monthly?.resetsAt, percent: percent)
            sessionControls(
                .opencode,
                needsSignIn: openCodeAuth.needsSignIn,
                hasData: snapshot != nil,
                signIn: { openSignIn(.opencode) },
                signOut: { openCodeAuth.signOut() }
            )
        }
    }

    private var cursorUsageCard: some View {
        let snapshot = cursorPoller.snapshot
        let percent = snapshot?.usedPercent ?? 0
        return usageCard(.cursor, period: "Monthly", percent: percent) {
            limitCaption(.cursor, hasData: snapshot != nil, resetsAt: snapshot?.resetsAt, percent: percent)
            sessionControls(
                .cursor,
                needsSignIn: cursorAuth.needsSignIn,
                hasData: snapshot != nil,
                signIn: { openSignIn(.cursor) },
                signOut: { cursorAuth.signOut() }
            )
        }
    }

    private var claudeUsageCard: some View {
        let snapshot = claudePoller.snapshot
        let percent = snapshot?.sevenDay?.usedPercent ?? 0
        return usageCard(.claude, period: "Weekly", percent: percent) {
            limitCaption(.claude, hasData: snapshot != nil, resetsAt: snapshot?.sevenDay?.resetsAt, percent: percent)
            sessionControls(
                .claude,
                needsSignIn: claudeAuth.needsSignIn,
                hasData: snapshot != nil,
                signIn: { openSignIn(.claude) },
                signOut: { claudeAuth.signOut() }
            )
        }
    }

    private var chatGPTUsageCard: some View {
        let snapshot = chatGPTPoller.snapshot
        let percent = snapshot?.headlineUsedPercent ?? 0
        return usageCard(.chatgpt, period: "Codex", percent: percent) {
            limitCaption(.chatgpt, hasData: snapshot != nil, resetsAt: snapshot?.resetsAt, percent: percent)
            sessionControls(
                .chatgpt,
                needsSignIn: chatGPTAuth.needsSignIn,
                hasData: snapshot != nil,
                signIn: { openSignIn(.chatgpt) },
                signOut: { chatGPTAuth.signOut() }
            )
        }
    }

    private var grokbotUsageCard: some View {
        let snapshot = grokbotPoller.snapshot
        let percent = snapshot?.usedPercent ?? 0
        // Grokbot bills through Cursor, so its session lives on the Cursor card.
        let showsSignIn = cursorAuth.needsSignIn && snapshot == nil
        return usageCard(.grokbot, period: "Weekly", percent: percent) {
            limitCaption(.grokbot, hasData: snapshot != nil, resetsAt: snapshot?.resetsAt, percent: percent)
            sessionControls(
                .grokbot,
                signIn: showsSignIn ? { openSignIn(.cursor) } : nil,
                signOut: nil,
                signInTitle: "Sign In to Cursor…"
            )
        }
    }

    private var openRouterUsageCard: some View {
        let snapshot = openRouterPoller.snapshot
        let percent = snapshot?.usedPercent ?? 0
        return usageCard(
            .openrouter,
            period: "Credits",
            headline: Self.openRouterHeadline(snapshot),
            percent: percent
        ) {
            if let snapshot, snapshot.budgetSource == nil {
                captionText("No credit limit — \(Format.usd(snapshot.keyUsageUSD)) spent on this key")
            } else if let remaining = snapshot?.remainingUSD {
                UsageLimitCaption(
                    resetsAt: snapshot?.resetsAt,
                    percent: percent,
                    normalCaption: "\(Format.usd(remaining)) of credits left"
                )
            } else if snapshot == nil {
                waitingCaption(.openrouter)
            }
            sessionControls(
                .openrouter,
                needsSignIn: openRouterAuth.needsSignIn,
                hasData: snapshot != nil,
                signIn: selectOpenRouter,
                signOut: { openRouterAuth.signOut() }
            )
        }
    }

    // MARK: - Card parts

    /// "42% Used" for a card's headline.
    static func usedLabel(_ percent: Double) -> String {
        "\(Int(percent.rounded()))% Used"
    }

    /// OpenRouter's headline: the key's spend when it has no credit limit to
    /// measure a percent against, otherwise the percent used.
    static func openRouterHeadline(_ snapshot: OpenRouterSnapshot?) -> String {
        if let snapshot, snapshot.usedPercent == nil {
            return Format.usd(snapshot.keyUsageUSD)
        }
        return usedLabel(snapshot?.usedPercent ?? 0)
    }

    /// A provider card: logo, name and period, headline, usage bar, then the
    /// caption and session controls in `content`.
    private func usageCard(
        _ provider: MonitorProvider,
        period: String,
        headline: String? = nil,
        percent: Double,
        barColor: Color? = nil,
        @ViewBuilder content: () -> some View
    ) -> some View {
        PanelCard {
            HStack(alignment: .center, spacing: 8) {
                Image(nsImage: ProviderLogo.image(for: provider))
                    .resizable()
                    .interpolation(.high)
                    .scaledToFit()
                    .frame(width: 14, height: 14)
                Text(provider.displayName)
                    .font(PanelTypography.title)
                    .foregroundStyle(.primary)
                Text("— " + period).panelMetaLabel(uppercased: true)
                Spacer()
                Text(headline ?? Self.usedLabel(percent))
                    .font(PanelTypography.bodyDigit)
                    .foregroundStyle(.primary)
            }
            GeometryReader { geo in
                let fillWidth = max(0, geo.size.width * CGFloat(Percent.clamp(percent) / 100))
                ZStack(alignment: .leading) {
                    Capsule().fill(Color.primary.opacity(0.12))
                    Capsule().fill(barColor ?? provider.accentColor).frame(width: fillWidth)
                }
            }
            .frame(height: 8)
            content()
        }
    }

    /// The reset caption once data has loaded, otherwise "No … data yet".
    @ViewBuilder
    private func limitCaption(_ provider: MonitorProvider, hasData: Bool, resetsAt: Date?, percent: Double) -> some View {
        if hasData {
            UsageLimitCaption(resetsAt: resetsAt, percent: percent)
        } else {
            waitingCaption(provider)
        }
    }

    private func waitingCaption(_ provider: MonitorProvider) -> some View {
        captionText("No " + provider.displayName + " data yet")
    }

    private func captionText(_ text: String) -> some View {
        Text(text)
            .font(PanelTypography.caption)
            .foregroundStyle(.tertiary)
    }

    /// Sign-in while the session needs one and no data has loaded; sign-out
    /// while the session is live.
    private func sessionControls(
        _ provider: MonitorProvider,
        needsSignIn: Bool,
        hasData: Bool,
        signIn: @escaping () -> Void,
        signOut: @escaping () -> Void
    ) -> some View {
        sessionControls(
            provider,
            signIn: needsSignIn && !hasData ? signIn : nil,
            signOut: needsSignIn ? nil : signOut
        )
    }

    @ViewBuilder
    private func sessionControls(
        _ provider: MonitorProvider,
        signIn: (() -> Void)?,
        signOut: (() -> Void)?,
        signInTitle: String? = nil
    ) -> some View {
        if let signIn {
            ProviderSignInButton(provider: provider, title: signInTitle, action: signIn)
        }
        if let signOut {
            ProviderSignOutButton(provider: provider, action: signOut)
        }
    }
}

/// The hourly usage chart for today or an earlier saved day, with arrows to
/// step between days.
struct OverviewDailyUsageCard: View {
    let now: Date
    let history: OverviewHourlyHistoryStore
    let todaySeries: (Date) -> OverviewDaySeries
    var calendar: Calendar = .current

    /// 0 is today; -1 is yesterday.
    @State private var dayOffset: Int

    /// - Parameter dayOffset: The day shown first, 0 for today.
    init(
        now: Date,
        history: OverviewHourlyHistoryStore,
        todaySeries: @escaping (Date) -> OverviewDaySeries,
        calendar: Calendar = .current,
        dayOffset: Int = 0
    ) {
        self.now = now
        self.history = history
        self.todaySeries = todaySeries
        self.calendar = calendar
        _dayOffset = State(initialValue: dayOffset)
    }

    private var day: Date {
        let today = calendar.startOfDay(for: now)
        return calendar.addingDays(dayOffset, to: today)
    }

    private var series: OverviewDaySeries? {
        dayOffset == 0 ? todaySeries(now) : history.series(on: day)
    }

    var body: some View {
        let day = day
        let usage = series.map { ProviderDayHourlyUsage.build(dayStart: day, series: $0) }
        PanelCard {
            PanelSectionHeader(title: Self.title(dayOffset: dayOffset, day: day))
            OverviewHourlyUsageChart(
                usage: usage?.isEmpty == false ? usage : nil,
                emptyMessage: dayOffset == 0
                    ? "No Grok, Grokbot, OpenCode, Cursor, or Claude activity yet today."
                    : "No activity recorded on this day."
            )
            if Self.canGoBack(from: day, earliestDay: history.earliestDay, calendar: calendar) || dayOffset < 0 {
                WeekChartNavigation(
                    onPreviousWeek: Self.canGoBack(from: day, earliestDay: history.earliestDay, calendar: calendar)
                        ? { dayOffset -= 1 }
                        : nil,
                    onNextWeek: { dayOffset = min(0, dayOffset + 1) },
                    canGoNext: dayOffset < 0
                )
            }
        }
    }

    /// "Usage Today", "Usage Yesterday", or "Usage Mon, Oct 20".
    static func title(dayOffset: Int, day: Date) -> String {
        switch dayOffset {
        case 0: return "Usage Today"
        case -1: return "Usage Yesterday"
        default: return "Usage \(Format.date(day, dateFormat: "EEE, MMM d"))"
        }
    }

    /// True when a saved day exists before `day`.
    static func canGoBack(from day: Date, earliestDay: Date?, calendar: Calendar) -> Bool {
        guard let earliestDay else { return false }
        return calendar.startOfDay(for: earliestDay) < calendar.startOfDay(for: day)
    }
}
