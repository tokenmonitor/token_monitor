@testable import TokenMon
import XCTest

/// OpenCode value types the panel and Overview read: window and model ids
/// and names, hourly totals and labels, the monthly headline fallback, and
/// the local-database error messages.
final class OpenCodeModelsTests: XCTestCase {
    private func segment(_ model: String, cost: Double, tokens: Int64) -> OpenCodeHourSegment {
        OpenCodeHourSegment(
            providerID: "opencode-go",
            modelID: model,
            costUSD: cost,
            quotaCostUSD: cost,
            inputTokens: tokens,
            outputTokens: 1,
            cacheReadTokens: 2,
            cacheWriteTokens: 3
        )
    }

    func testWindowAndModelIdentity() {
        let window = OpenCodeWindowUsage(kind: .weekly, usedUSD: 45, limitUSD: 30, resetsAt: nil, sessionCount: 1)
        XCTAssertEqual(window.id, .weekly)
        XCTAssertEqual(window.usedPercent, 100, "over the limit clamps to 100%")
        XCTAssertEqual(OpenCodeWindowUsage.clampedPercent(usedUSD: 5, limitUSD: 0), 0)
        XCTAssertEqual(OpenCodeWindowKind.allCases.map(\.label), ["5-Hour", "Weekly", "Monthly"])

        let model = OpenCodeModelUsage(
            providerID: "opencode-go",
            modelID: "kimi-k3",
            sessionCount: 1,
            inputTokens: 0,
            outputTokens: 0,
            cacheReadTokens: 0,
            cacheWriteTokens: 0,
            costUSD: 0,
            percentOfWindow: 0
        )
        XCTAssertEqual(model.id, "opencode-go/kimi-k3")
        XCTAssertEqual(model.displayName, OpenCodeCatalog.modelDisplayName(providerID: "opencode-go", modelID: "kimi-k3"))
    }

    func testHourTotalsAndLabels() {
        let one = segment("a", cost: 0.5, tokens: 10)
        XCTAssertEqual(one.id, "opencode-go/a")
        XCTAssertEqual(one.totalTokens, 16)
        XCTAssertEqual(one.messageCount, 0)
        XCTAssertFalse(one.displayName.isEmpty)
        let hour = OpenCodeHourUsage(hour: 14, segments: [one, segment("b", cost: 0.25, tokens: 4)])
        XCTAssertEqual(hour.id, 14)
        XCTAssertEqual(hour.totalUSD, 0.75, accuracy: 0.0001)
        XCTAssertEqual(hour.totalTokens, 26)
        XCTAssertEqual(hour.hourLabel, Format.hourLabel(for: 14))
    }

    func testDayIsEmptyOnlyWithoutCostAndMessages() {
        let quiet = (0..<24).map { OpenCodeHourUsage(hour: $0, segments: []) }
        var day = OpenCodeDayHourlyUsage(dayStart: Date(), hours: quiet, legend: [])
        XCTAssertEqual(day.maxHourUSD, 0)
        XCTAssertTrue(day.isEmpty)
        day.hours[3].messageCount = 2
        XCTAssertFalse(day.isEmpty, "messages without cost still count as activity")
        day.hours[3] = OpenCodeHourUsage(hour: 3, segments: [segment("a", cost: 1.25, tokens: 1)])
        XCTAssertEqual(day.maxHourUSD, 1.25, accuracy: 0.0001)
        XCTAssertFalse(day.isEmpty)
    }

    func testMonthlyHeadlineFallsBackToTheWeeklyWindow() {
        let weekly = OpenCodeWindowUsage(kind: .weekly, usedUSD: 15, limitUSD: 30, resetsAt: nil, sessionCount: 0)
        XCTAssertEqual(OpenCodeSnapshot(windows: [weekly], models: []).monthlyUsedPercent, 50, accuracy: 0.001)
        XCTAssertEqual(OpenCodeSnapshot(windows: [], models: []).primaryUsedPercent, 0)
        XCTAssertFalse(OpenCodeSnapshot(windows: [], models: []).hasLocalStats)
    }

    func testMuseSparkAliasIsNamedByProvider() {
        XCTAssertEqual(OpenCodeCatalog.modelDisplayName(providerID: "anthropic", modelID: "Muse_Spark-1"), "Anthropic · Muse Spark")
    }

    func testLocalDatabaseErrorsExplainTheFailure() {
        let url = URL(fileURLWithPath: "/tmp/none/opencode.db")
        XCTAssertEqual(
            OpenCodeLocalStatsError.databaseMissing(url).localizedDescription,
            "OpenCode usage database not found at /tmp/none/opencode.db. Install and use OpenCode to track usage."
        )
        XCTAssertEqual(OpenCodeLocalStatsError.openFailed("x").localizedDescription, "Could not open the OpenCode usage database: x")
        XCTAssertEqual(OpenCodeLocalStatsError.queryFailed("y").localizedDescription, "Could not read OpenCode usage: y")
    }

    func testOpeningAMissingDatabaseFails() {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).appendingPathComponent("opencode.db")
        XCTAssertThrowsError(try OpenCodeLocalStats.openConnection(at: url)) { error in
            guard case OpenCodeLocalStatsError.openFailed = error else { return XCTFail("unexpected \(error)") }
        }
    }
}
