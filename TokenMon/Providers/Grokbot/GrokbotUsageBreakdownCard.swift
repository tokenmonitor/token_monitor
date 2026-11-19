import SwiftUI

/// Grokbot "Usage Breakdown" card: who spent the allowance (You, Routines,
/// Bot to Bot, …), most used first. Percent slices are percents of the weekly
/// pool. Dollar slices split `consumedPercent` of the usage bar.
struct GrokbotUsageBreakdownCard: View {
    let breakdown: GrokbotUsageBreakdown
    /// Usage-bar percent filled by dollar slices. Percent slices ignore it.
    var consumedPercent: Double?

    var body: some View {
        UsageBreakdownCard(
            items: breakdown.slices.map { slice in
                UsageBreakdownCard.Item(
                    id: slice.source.rawValue,
                    label: slice.source.displayName,
                    color: slice.source.sRGB.color,
                    amount: slice.value,
                    // On-demand accounts read better in dollars than as shares.
                    valueText: breakdown.unit == .usd ? Format.usd(slice.value) : nil
                )
            },
            consumedPercent: breakdown.unit == .usd ? consumedPercent : nil
        )
    }
}

#if DEBUG
#Preview {
    GrokbotUsageBreakdownCard(
        breakdown: GrokbotUsageBreakdown(
            unit: .weeklyLimitPercent,
            slices: [
                .init(source: .user, value: 50.31),
                .init(source: .automation, value: 28.09),
                .init(source: .agent, value: 21.6)
            ]
        )
    )
    .padding()
    .frame(width: 340)
}
#endif
