import SwiftUI

/// "Saved resets" card at the bottom of a provider panel: how many
/// usage-limit resets the account can spend, and when the next one expires.
/// Renders nothing without an inventory.
struct SavedResetsCard: View {
    let resets: SavedResets?

    var body: some View {
        if let resets {
            PanelCard {
                HStack {
                    Text("Saved resets")
                    Spacer()
                    Text(resets.countLabel)
                        .font(PanelTypography.bodyDigit)
                }
                .font(PanelTypography.body)
                .foregroundStyle(.primary)
                if let expiry = resets.nextExpiry {
                    Text("Next expires \(Format.resetDate(expiry, dateFormat: "EEE dd MMMM h:mma"))")
                        .resetCaption()
                }
            }
        }
    }
}

#if DEBUG
#Preview {
    VStack(spacing: 10) {
        SavedResetsCard(resets: SavedResets(count: 2, nextExpiry: Date().addingTimeInterval(9 * 86_400)))
        SavedResetsCard(resets: SavedResets(count: 1))
    }
    .padding()
    .frame(width: 380)
}
#endif
