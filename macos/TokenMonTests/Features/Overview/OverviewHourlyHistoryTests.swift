@testable import TokenMon
import XCTest

@MainActor
final class OverviewHourlyHistoryTests: XCTestCase {
    private var directory: URL!
    private var calendar: Calendar!

    override func setUpWithError() throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("OverviewHistory-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        var utc = Calendar(identifier: .gregorian)
        utc.timeZone = TimeZone(identifier: "UTC") ?? utc.timeZone
        calendar = utc
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: directory)
    }

    private func makeStore() -> OverviewHourlyHistoryStore {
        OverviewHourlyHistoryStore(
            store: FileBackedStringStore(directory: directory, filenamePrefix: "activity_"),
            calendar: calendar
        )
    }

    private func date(_ day: Int, month: Int = 10, hour: Int = 12) throws -> Date {
        try XCTUnwrap(calendar.date(from: DateComponents(year: 2026, month: month, day: day, hour: hour)))
    }

    private func hours(_ values: [Int: Double]) -> [Double] {
        (0..<24).map { values[$0] ?? 0 }
    }

    private func series(_ grok: [Int: Double] = [:], claude: [Int: Double] = [:]) -> OverviewDaySeries {
        var series = OverviewDaySeries()
        if !grok.isEmpty { series.weights[.grok] = hours(grok) }
        if !claude.isEmpty { series.weights[.claude] = hours(claude) }
        return series
    }

    // MARK: - OpenCode contribution

    private func openCodeHourly(dayStart: Date) -> OpenCodeDayHourlyUsage {
        let hours = (0..<24).map { hour in
            OpenCodeHourUsage(
                hour: hour,
                segments: hour == 9 ? [
                    OpenCodeHourSegment(
                        providerID: "opencode-go",
                        modelID: "m",
                        costUSD: 1,
                        quotaCostUSD: 1,
                        inputTokens: 100,
                        messageCount: 1
                    )
                ] : []
            )
        }
        return OpenCodeDayHourlyUsage(dayStart: dayStart, hours: hours, legend: [])
    }

    /// OpenCode's local hours are added for the matching day; the gate is the
    /// provider being enabled, not a console session.
    func testOpenCodeHoursAreAddedForTheSameDay() throws {
        let dayStart = calendar.startOfDay(for: try date(5))
        var series = OverviewDaySeries()
        series.addOpenCode(openCodeHourly(dayStart: dayStart), monthlyLimitUSD: 60, dayStart: dayStart, calendar: calendar)
        let goWeights = try XCTUnwrap(series.weights[.openCodeGo])
        XCTAssertGreaterThan(goWeights[9], 0)
        XCTAssertEqual(series.tokens[.openCodeGo]?[9], 100)
        XCTAssertTrue(series.hasActivity)
    }

    func testOpenCodeHoursFromAnotherDayAreIgnored() throws {
        let dayStart = calendar.startOfDay(for: try date(5))
        let otherDay = calendar.startOfDay(for: try date(4))
        var series = OverviewDaySeries()
        series.addOpenCode(openCodeHourly(dayStart: otherDay), monthlyLimitUSD: 60, dayStart: dayStart, calendar: calendar)
        XCTAssertNil(series.weights[.openCodeGo])
        XCTAssertFalse(series.hasActivity)
    }

    // MARK: - History store

    func testRecordKeepsTheLargerValueForEachHour() throws {
        let store = makeStore()
        store.record(series([9: 2, 10: 1]), at: try date(22))
        store.record(series([9: 1, 10: 3], claude: [10: 4]), at: try date(22, hour: 15))

        let saved = try XCTUnwrap(store.series(on: try date(22)))
        XCTAssertEqual(saved.weights[.grok]?[9], 2)
        XCTAssertEqual(saved.weights[.grok]?[10], 3)
        XCTAssertEqual(saved.weights[.claude]?[10], 4)
    }

    func testAnEmptyDayIsNotSaved() throws {
        let store = makeStore()
        store.record(OverviewDaySeries(), at: try date(22))
        XCTAssertTrue(store.days.isEmpty)
        XCTAssertNil(store.earliestDay)
    }

    func testHistoryReloadsFromDisk() throws {
        makeStore().record(series([9: 2]), at: try date(21))
        let reloaded = makeStore()
        XCTAssertEqual(reloaded.series(on: try date(21))?.weights[.grok]?[9], 2)
        XCTAssertEqual(reloaded.earliestDay, try date(21, hour: 0))
    }

    func testDaysPastTheRetentionWindowAreDropped() throws {
        let store = makeStore()
        store.record(series([9: 1]), at: try date(1))
        store.record(series([9: 1]), at: try date(30))
        XCTAssertNotNil(store.series(on: try date(1)))

        // Thirty days including Oct 31 start on Oct 2, so Oct 1 is dropped.
        store.record(series([9: 1]), at: try date(31))
        XCTAssertNil(store.series(on: try date(1)))
        XCTAssertNotNil(store.series(on: try date(30)))
    }

    func testSeriesKeysAreSavedByName() throws {
        let data = try JSONEncoder().encode(series([0: 1]))
        let text = try XCTUnwrap(String(bytes: data, encoding: .utf8))
        XCTAssertTrue(text.contains("\"grok\""))
        XCTAssertEqual(try JSONDecoder().decode(OverviewDaySeries.self, from: data), series([0: 1]))
        XCTAssertNil(OverviewSeries(rawValue: "unknown"))
        XCTAssertEqual(OverviewSeries(rawValue: OverviewSeries.openCodeZen.rawValue), .openCodeZen)
    }

    // MARK: - Chart scaling

    func testSegmentHeightsAreProportionalAndSumToTheBar() {
        let heights = OverviewHourlyUsageChart.segmentHeights([1, 3], total: 40, minimum: 1)
        XCTAssertEqual(heights[0], 10, accuracy: 0.001)
        XCTAssertEqual(heights[1], 30, accuracy: 0.001)
    }

    func testThinSlicesAreLiftedWithoutOverflowingTheBar() {
        let heights = OverviewHourlyUsageChart.segmentHeights([0.001, 0.002, 10], total: 20, minimum: 1)
        XCTAssertEqual(heights.reduce(0, +), 20, accuracy: 0.001)
        XCTAssertEqual(heights[0], 1, accuracy: 0.001)
        XCTAssertEqual(heights[1], 1, accuracy: 0.001)
        XCTAssertEqual(heights[2], 18, accuracy: 0.001)
    }

    func testSlicesStayProportionalWhenTheBarIsTooShortForMinimums() {
        let heights = OverviewHourlyUsageChart.segmentHeights([1, 1, 2], total: 2, minimum: 1)
        XCTAssertEqual(heights.reduce(0, +), 2, accuracy: 0.001)
        XCTAssertEqual(heights[2], 1, accuracy: 0.001)
    }

    func testBarHeightFollowsActivityWithAVisibleMinimum() {
        XCTAssertEqual(OverviewHourlyUsageChart.barHeight(activity: 5, maxActivity: 10, trackHeight: 60), 30)
        XCTAssertEqual(OverviewHourlyUsageChart.barHeight(activity: 10, maxActivity: 10, trackHeight: 60), 60)
        XCTAssertEqual(OverviewHourlyUsageChart.barHeight(activity: 0.001, maxActivity: 10, trackHeight: 60), 3)
        XCTAssertEqual(OverviewHourlyUsageChart.barHeight(activity: 0, maxActivity: 10, trackHeight: 60), 0)
    }

    func testVisibleHoursWidenToIncludeLateAndEarlyActivity() {
        XCTAssertEqual(OverviewHourlyUsageChart.visibleHourRange(activeHours: [10]), 6...22)
        XCTAssertEqual(OverviewHourlyUsageChart.visibleHourRange(activeHours: [2, 23]), 2...23)
    }

    func testAxisLabelsMatchTheDefaultDayAndNeverCrowd() {
        XCTAssertEqual(OverviewHourlyUsageChart.axisHours(for: 6...22), [6, 9, 12, 15, 18, 22])
        XCTAssertEqual(OverviewHourlyUsageChart.axisHours(for: 2...23), [2, 6, 9, 12, 15, 18, 21, 23])
    }

    // MARK: - Day navigation

    func testDayTitles() throws {
        let day = try date(20)
        XCTAssertEqual(OverviewDailyUsageCard.title(dayOffset: 0, day: day), "Usage Today")
        XCTAssertEqual(OverviewDailyUsageCard.title(dayOffset: -1, day: day), "Usage Yesterday")
        XCTAssertTrue(OverviewDailyUsageCard.title(dayOffset: -2, day: day).hasPrefix("Usage "))
    }

    func testBackArrowNeedsAnEarlierSavedDay() throws {
        XCTAssertFalse(OverviewDailyUsageCard.canGoBack(from: try date(22), earliestDay: nil, calendar: calendar))
        XCTAssertFalse(OverviewDailyUsageCard.canGoBack(from: try date(22), earliestDay: try date(22, hour: 0), calendar: calendar))
        XCTAssertTrue(OverviewDailyUsageCard.canGoBack(from: try date(22), earliestDay: try date(21, hour: 0), calendar: calendar))
    }

    // MARK: - Series merging and chart hours

    func testMergingKeepsTheLargerTokenCountsAndPadsShortDays() {
        var lhs = OverviewDaySeries()
        lhs.tokens[.cursor] = [5, 1]
        var rhs = OverviewDaySeries()
        rhs.tokens[.cursor] = [3, 4, 9]
        rhs.tokens[.openCodeGo] = [2]
        let merged = lhs.merging(rhs)
        XCTAssertEqual(merged.tokens[.cursor], [5, 4, 9])
        XCTAssertEqual(merged.tokens[.openCodeGo], [2])
        XCTAssertFalse(merged.hasActivity)
    }

    func testChartHoursComeFromEachSeries() throws {
        var series = OverviewDaySeries()
        series.weights[.grokbot] = hours([8: 1])
        series.weights[.cursor] = hours([8: 3])
        series.tokens[.cursor] = (0..<24).map { $0 == 8 ? 1_000 : 0 }
        let usage = ProviderDayHourlyUsage.build(dayStart: try date(5, hour: 0), series: series)
        XCTAssertEqual(usage.hours[8].activity, 4, accuracy: 0.001)
        XCTAssertEqual(usage.hours[8].grokbotActivity, 1, accuracy: 0.001)
        XCTAssertEqual(usage.hours[8].cursorTokens, 1_000)
        XCTAssertNil(usage.hours[8].grokTokens)
        XCTAssertTrue(ProviderDayHourlyUsage.build(dayStart: try date(5, hour: 0), series: OverviewDaySeries()).isEmpty)
    }
}

/// Today's Overview inputs come from the providers that are enabled and
/// signed in; OpenCode's local hours need only the provider enabled.
@MainActor
final class OverviewTodaySourcesTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var defaults: UserDefaults!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "OverviewSources-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suiteName)
    }

    private func hours(_ values: [Int: Double]) -> [Double] {
        (0..<24).map { values[$0] ?? 0 }
    }

    /// A fixture whose hourly stores each grew 5 points this hour, with Cursor
    /// reporting 2 points and 900 tokens at 9am and OpenCode $1 of Go at 9am.
    private func busyFixture() -> OverviewPanelFixture {
        let fixture = OverviewPanelFixture(directory: dir, defaults: defaults)
        for store in [fixture.grokHourly, fixture.claudeHourly, fixture.grokbotHourly] {
            store.record(usedPercent: 10)
            store.record(usedPercent: 15)
        }
        let today = Calendar.current.startOfDay(for: Date())
        fixture.cursorHourly = CursorDayHourlyUsage(
            dayStart: today,
            hourWeights: hours([9: 1]),
            quotaHourWeights: hours([9: 2]),
            hourTokenWeights: (0..<24).map { $0 == 9 ? 900 : 0 }
        )
        let openCodeHours = (0..<24).map { hour in
            OpenCodeHourUsage(hour: hour, segments: hour == 9 ? [
                OpenCodeHourSegment(
                    providerID: "opencode-go",
                    modelID: "m",
                    costUSD: 1,
                    quotaCostUSD: 1,
                    inputTokens: 100,
                    messageCount: 1
                )
            ] : [])
        }
        fixture.openCodeHourly = OpenCodeDayHourlyUsage(dayStart: today, hours: openCodeHours, legend: [])
        return fixture
    }

    private func total(_ weights: [Double]?) -> Double {
        weights?.reduce(0, +) ?? 0
    }

    func testSignedInProvidersContributeTodaysHours() async throws {
        let fixture = busyFixture()
        fixture.signInAll()
        await fixture.cursorPoller.refreshNow()
        await fixture.openCodePoller.refreshNow()
        let series = fixture.sources.series(on: Date())

        XCTAssertEqual(total(series.weights[.grok]), 5, accuracy: 0.001)
        XCTAssertEqual(total(series.weights[.claude]), 5, accuracy: 0.001)
        XCTAssertEqual(total(series.weights[.grokbot]), 5, accuracy: 0.001)
        XCTAssertEqual(series.weights[.cursor]?[9] ?? 0, 2, accuracy: 0.001)
        XCTAssertEqual(series.tokens[.cursor]?[9], 900)
        let go = try XCTUnwrap(series.weights[.openCodeGo])
        XCTAssertEqual(go[9], 1 / (60 / QuotaNormalization.averageWeeksPerMonth) * 100, accuracy: 0.001)
        XCTAssertEqual(series.tokens[.openCodeGo]?[9], 100)
    }

    /// Signed-out sessions contribute nothing, but OpenCode's local hours
    /// still count, measured against the default monthly limit when the
    /// local read gives no plan limit.
    func testSignedOutProvidersContributeOnlyOpenCode() async throws {
        let fixture = busyFixture()
        fixture.openCodeLimitUSD = 240
        fixture.openCodeHasLocalSnapshot = false
        await fixture.cursorPoller.refreshNow()
        await fixture.openCodePoller.refreshNow()
        XCTAssertNil(fixture.openCodePoller.snapshot)
        let series = fixture.sources.series(on: Date())
        XCTAssertNil(series.weights[.grok])
        XCTAssertNil(series.weights[.claude])
        XCTAssertNil(series.weights[.grokbot])
        XCTAssertNil(series.weights[.cursor])
        let go = try XCTUnwrap(series.weights[.openCodeGo])
        let defaultLimit = OpenCodeWindowKind.monthly.defaultLimitUSD
        XCTAssertEqual(go[9], 1 / (defaultLimit / QuotaNormalization.averageWeeksPerMonth) * 100, accuracy: 0.001)
    }

    /// The OpenCode plan's own monthly limit scales its hours.
    func testOpenCodeHoursFollowThePlanLimit() async throws {
        let fixture = busyFixture()
        fixture.openCodeLimitUSD = 120
        await fixture.openCodePoller.refreshNow()
        let go = try XCTUnwrap(fixture.sources.series(on: Date()).weights[.openCodeGo])
        XCTAssertEqual(go[9], 1 / (120 / QuotaNormalization.averageWeeksPerMonth) * 100, accuracy: 0.001)
    }

    func testDisabledProvidersContributeNothing() async {
        let fixture = busyFixture()
        fixture.signInAll()
        await fixture.cursorPoller.refreshNow()
        await fixture.openCodePoller.refreshNow()
        fixture.settings.enabledProviderIDs = [.chatgpt]
        XCTAssertEqual(fixture.sources.series(on: Date()), OverviewDaySeries())
    }

    /// Hours recorded for another day are not today's.
    func testHoursFromAnotherDayAreIgnored() async throws {
        let fixture = busyFixture()
        fixture.signInAll()
        await fixture.cursorPoller.refreshNow()
        await fixture.openCodePoller.refreshNow()
        let tomorrow = try XCTUnwrap(Calendar.current.date(byAdding: .day, value: 1, to: Date()))
        XCTAssertEqual(fixture.sources.series(on: tomorrow), OverviewDaySeries())
    }

    /// Cursor hours of the wrong length are left out rather than misplaced.
    func testMalformedCursorHoursAreIgnored() async {
        let fixture = busyFixture()
        fixture.signInAll()
        fixture.cursorHourly.quotaHourWeights = [1]
        fixture.cursorHourly.hourTokenWeights = [1]
        await fixture.cursorPoller.refreshNow()
        let series = fixture.sources.series(on: Date())
        XCTAssertNil(series.weights[.cursor])
        XCTAssertNil(series.tokens[.cursor])
    }

    /// The app model reads the same sources from its own sessions and stores.
    func testAppModelSeriesReadsItsOwnSessions() {
        let model = AppModel()
        XCTAssertNil(model.overviewSeries(on: Date()).weights[.grok])
        model.grokAuth.save(cookieHeader: "sso=live")
        model.grokHourly.record(usedPercent: 10)
        model.grokHourly.record(usedPercent: 12)
        XCTAssertNotNil(model.overviewSeries(on: Date()).weights[.grok])
        XCTAssertIdentical(model.overviewTodaySources.cursorPoller, model.cursorPoller)
    }
}
