import SwiftUI

/// "Usage Breakdown" card shared by the provider panels: one bar split by what
/// spent the pool, then a legend of the same items, most used first.
///
/// An item with no `valueText` is already a percent of the usage bar. Those
/// slices keep that percent, so they sum to the consumed part of the bar and
/// the unused pool stays grey. An item with `valueText` is a quantity (dollars);
/// its slice is its share of the quantities, filling `consumedPercent` of the
/// bar when that is the usage bar's percent, or the whole bar when it is nil.
struct UsageBreakdownCard: View {
    /// One slice of the breakdown.
    struct Item: Identifiable {
        var id: String
        var label: String
        var color: Color
        /// Percent of the usage bar, or a quantity when `valueText` is set.
        var amount: Double
        /// The legend's figure (e.g. "$1.25"); nil shows `amount` as a percent of the pool.
        var valueText: String?
    }

    /// Items in display order (most used first).
    let items: [Item]
    /// Usage-bar percent that dollar quantities fill. Ignored when every item
    /// is already a percent of the pool.
    var consumedPercent: Double?

    var body: some View {
        let amounts = items.map { max(0, $0.amount) }
        let poolPercents = items.allSatisfy { $0.valueText == nil } ? Self.poolPercents(amounts) : nil
        let segmentPercents = Self.segmentPercents(amounts, poolPercents: poolPercents, consumedPercent: consumedPercent)
        let segments = zip(items, segmentPercents).map { item, percent in
            SegmentedUsageBar.Segment(id: item.id, percent: percent, color: item.color)
        }
        PanelCard {
            PanelSectionHeader(title: "Usage Breakdown")
            SegmentedUsageBar(segments: segments, height: 8)
            // Chips keep their natural width and wrap, so no name is cut off.
            FlowLayout {
                ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                    UsageLegendChip(
                        color: item.color,
                        label: item.label,
                        value: item.valueText ?? "\(poolPercents?[index] ?? 0)%"
                    )
                }
            }
        }
    }

    /// Bar widths. Pool percents pass through. Quantities become shares of
    /// `consumedPercent`, or of the whole bar when that percent is absent.
    static func segmentPercents(
        _ amounts: [Double],
        poolPercents: [Int]?,
        consumedPercent: Double?
    ) -> [Double] {
        if poolPercents != nil { return amounts }
        let total = amounts.reduce(0, +)
        guard total > 0 else { return amounts.map { _ in 0 } }
        let scale = consumedPercent ?? 100
        return amounts.map { $0 / total * max(0, scale) }
    }

    /// Whole-number percents of the pool that add up to the rounded sum of
    /// `amounts` (largest remainder), so a 25 / 12 / 6 / 4 split stays 47
    /// rather than being stretched to 100. All zeros when nothing was used.
    /// Negative amounts count as zero.
    static func poolPercents(_ amounts: [Double]) -> [Int] {
        let clamped = amounts.map { max(0, $0) }
        let total = clamped.reduce(0, +)
        guard total > 0 else { return Array(repeating: 0, count: clamped.count) }
        let target = Int(total.rounded())
        guard target > 0 else { return Array(repeating: 0, count: clamped.count) }
        let exact = clamped.map { $0 / total * Double(target) }
        var shares = exact.map { Int($0.rounded(.down)) }
        let missing = target - shares.reduce(0, +)
        guard missing > 0 else { return shares }
        let byRemainder = exact.indices.sorted { lhs, rhs in
            let left = exact[lhs] - Double(shares[lhs])
            let right = exact[rhs] - Double(shares[rhs])
            return left != right ? left > right : lhs < rhs
        }
        for index in byRemainder.prefix(missing) {
            shares[index] += 1
        }
        return shares
    }
}

/// Legend chip for a segmented usage bar: color mark, name, and value on one
/// line at the same size (12pt semibold), the name grey and the value primary
/// so the numbers stand out, as under the daily budget bars.
struct UsageLegendChip: View {
    let color: Color
    let label: String
    let value: String

    var body: some View {
        HStack(spacing: 5) {
            Circle()
                .fill(color)
                .frame(width: 7, height: 7)
            Text(label)
                .font(PanelTypography.bodySemibold)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Text(value)
                .font(PanelTypography.bodyDigit)
                .foregroundStyle(.primary)
                .fixedSize()
        }
    }
}
