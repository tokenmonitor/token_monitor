import SwiftUI

/// Compact capsule label for a panel's footer row: an optional SF Symbol, a
/// short label and an optional figure, tinted text on a faint tinted capsule.
/// Sized to the caption scale so it sits on one line with the sign-out icon.
struct PanelPill: View {
    let label: String
    var systemImage: String?
    var value: String?
    var tint: Color = .accentColor

    var body: some View {
        HStack(spacing: 4) {
            if let systemImage {
                Image(systemName: systemImage)
            }
            Text(label)
            if let value {
                Text(value)
                    .font(PanelTypography.captionSemibold.monospacedDigit())
            }
        }
        .font(PanelTypography.caption)
        .foregroundStyle(tint)
        .padding(.horizontal, 8)
        .padding(.vertical, 2)
        .background(Capsule().fill(tint.opacity(0.14)))
        .fixedSize()
    }
}

/// "Resets available" pill with the count of saved usage-limit resets.
/// Hovering it shows when the next reset expires, when the provider reports
/// one. Renders nothing without an inventory.
struct SavedResetsPill: View {
    let resets: SavedResets?
    let tint: Color

    var body: some View {
        if let resets {
            PanelPill(
                label: "Resets available",
                systemImage: "arrow.counterclockwise",
                value: "\(resets.count)",
                tint: tint
            )
            .help(Self.helpText(for: resets))
            .accessibilityElement(children: .combine)
        }
    }

    private static func helpText(for resets: SavedResets) -> String {
        guard let expiry = resets.nextExpiry else { return "Saved usage-limit resets" }
        return "Next expires \(Format.resetDate(expiry, dateFormat: "EEE dd MMMM h:mma"))"
    }
}

#if DEBUG
#Preview {
    VStack(alignment: .leading, spacing: 10) {
        SavedResetsPill(
            resets: SavedResets(count: 2, nextExpiry: Date().addingTimeInterval(9 * 86_400)),
            tint: MonitorProvider.claude.accentColor
        )
        SavedResetsPill(resets: SavedResets(count: 1), tint: MonitorProvider.chatgpt.accentColor)
        PanelPill(label: "Update available", systemImage: "arrow.down.circle")
    }
    .padding()
    .frame(width: 380)
}
#endif
