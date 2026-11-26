import SwiftUI
@testable import TokenMon
import XCTest

/// The Overview hourly chart: the empty message, one legend entry per
/// provider with activity, axis labels, and each hour's tooltip.
@MainActor
final class OverviewHourlyUsageChartTests: XCTestCase {
    private func hours(_ values: [Int: Double]) -> [Double] {
        (0..<24).map { values[$0] ?? 0 }
    }

    private func tokens(_ values: [Int: Int64]) -> [Int64] {
        (0..<24).map { values[$0] ?? 0 }
    }

    /// Every provider busy at 9am, Grok also at midnight and 11pm.
    private func busyDay() -> ProviderDayHourlyUsage {
        ProviderDayHourlyUsage.build(
            dayStart: Date(),
            grokHourWeights: hours([0: 0.5, 9: 1, 23: 2]),
            openCodeGoHourWeights: hours([9: 0.25]),
            openCodeZenHourWeights: hours([9: 0.75]),
            cursorHourWeights: hours([9: 1.2]),
            claudeHourWeights: hours([9: 3]),
            grokbotHourWeights: hours([9: 0.4]),
            grokHourTokens: tokens([9: 2_000]),
            openCodeGoHourTokens: tokens([9: 1_500]),
            cursorHourTokens: tokens([9: 3_400_000])
        )
    }

    func testNoUsageShowsTheDefaultEmptyMessage() {
        let chart = OverviewHourlyUsageChart(usage: nil)
        ViewHarness.render(chart)
        XCTAssertTrue(ViewTree(chart).shows("No Grok, Grokbot, OpenCode, Cursor, or Claude activity yet today."))
    }

    /// Activity outside the visible hours still counts: a day whose only
    /// activity is zero shows the custom empty message.
    func testIdleDayShowsTheGivenEmptyMessage() {
        let idle = ProviderDayHourlyUsage.build(dayStart: Date(), grokHourWeights: [], openCodeGoHourWeights: [], openCodeZenHourWeights: [])
        let chart = OverviewHourlyUsageChart(usage: idle, emptyMessage: "Nothing here.")
        let tree = ViewTree(chart)
        XCTAssertTrue(tree.shows("Nothing here."))
        XCTAssertFalse(tree.shows("Grok"))
    }

    func testBusyDayShowsLegendAxisAndTooltips() {
        let chart = OverviewHourlyUsageChart(usage: busyDay())
        let busy = ViewHarness.renderBothAppearances(chart)
        let tree = ViewTree(chart)

        for legend in ["Grok", "Grokbot", "Cursor", "Claude", "Go", "Zen"] {
            XCTAssertTrue(tree.texts.contains(legend), legend)
        }
        // Midnight and 11pm widen the axis to the whole day: 12, 3, 6, 9, 12, 3, 6, 9, 11.
        XCTAssertTrue(tree.texts.contains("11"))
        XCTAssertTrue(tree.texts.contains("12"))
        XCTAssertTrue(tree.texts.contains("3"))

        // The 9am tooltip lists every slice bottom to top; local token counts say where.
        let nine = tree.texts.first { $0.hasPrefix("\(Format.hourLabel(for: 9)):") }
        let slices = [
            "Grok 1.00% of weekly · \(Format.tokens(2_000))",
            "Grokbot 0.40% of weekly",
            "Cursor 1.20% of weekly share · \(Format.tokens(3_400_000))",
            "Claude 3.00% of 5-hour window",
            "OpenCode Go 0.25% of weekly share · \(Format.tokens(1_500)) tokens \(UsageSource.thisComputer.locationPhrase ?? "")",
            "OpenCode Zen 0.75% of weekly share"
        ]
        XCTAssertEqual(nine, "\(Format.hourLabel(for: 9)): " + slices.joined(separator: " · "))

        let empty = ViewHarness.render(OverviewHourlyUsageChart(usage: nil))
        XCTAssertGreaterThan(busy.height, empty.height)
    }

    /// Only providers with activity in the visible hours get a legend entry.
    func testLegendListsOnlyActiveProviders() {
        let usage = ProviderDayHourlyUsage.build(
            dayStart: Date(),
            grokHourWeights: [],
            openCodeGoHourWeights: [],
            openCodeZenHourWeights: [],
            claudeHourWeights: hours([14: 1])
        )
        let tree = ViewTree(OverviewHourlyUsageChart(usage: usage))
        XCTAssertTrue(tree.texts.contains("Claude"))
        XCTAssertFalse(tree.texts.contains("Grok"))
        XCTAssertFalse(tree.texts.contains("Zen"))
    }

    // MARK: - Tooltip text

    private func segment(_ title: String, tokens: Int64?, source: UsageSource) -> OverviewHourlyUsageChart.HourBarSegment {
        OverviewHourlyUsageChart.HourBarSegment(
            id: title,
            title: title,
            activity: 1.2,
            tokens: tokens,
            tokenSource: source,
            color: .blue,
            quotaLabel: "of weekly share"
        )
    }

    func testSegmentHelpOmitsMissingOrZeroTokens() {
        XCTAssertEqual(
            OverviewHourlyUsageChart.segmentHelp(segment("Cursor", tokens: nil, source: .account)),
            "Cursor 1.20% of weekly share"
        )
        XCTAssertEqual(
            OverviewHourlyUsageChart.segmentHelp(segment("Cursor", tokens: 0, source: .account)),
            "Cursor 1.20% of weekly share"
        )
    }

    func testSegmentHelpSaysWhereTokensWereCounted() {
        XCTAssertEqual(
            OverviewHourlyUsageChart.segmentHelp(segment("Cursor", tokens: 3_400_000, source: .account)),
            "Cursor 1.20% of weekly share · \(Format.tokens(3_400_000))"
        )
        let local = OverviewHourlyUsageChart.segmentHelp(segment("OpenCode Go", tokens: 1_500, source: .thisComputer))
        XCTAssertEqual(
            local,
            "OpenCode Go 1.20% of weekly share · \(Format.tokens(1_500)) tokens \(UsageSource.thisComputer.locationPhrase ?? "")"
        )
        XCTAssertTrue(local.hasSuffix("on this \(UsageSource.computerName)"))
    }
}
