import SwiftUI

private struct UpdateCheckerKey: EnvironmentKey {
    static let defaultValue: UpdateChecker? = nil
}

extension EnvironmentValues {
    /// The app's update checker, read by `UpdateAvailableIndicator`.
    var updateChecker: UpdateChecker? {
        get { self[UpdateCheckerKey.self] }
        set { self[UpdateCheckerKey.self] = newValue }
    }
}

/// "Update available" pill for the bottom of the menu-bar panel. Appears
/// once the environment's update checker has found a newer release.
struct UpdateAvailableIndicator: View {
    @Environment(\.updateChecker) private var checker

    var body: some View {
        if let checker {
            UpdateAvailablePill(checker: checker)
        }
    }
}

/// The pill installs the release when pressed; its tooltip names the version.
/// A status message from the checker shows as a caption beneath it.
struct UpdateAvailablePill: View {
    let checker: UpdateChecker

    var body: some View {
        if let release = checker.availableRelease {
            VStack(alignment: .leading, spacing: 4) {
                Button {
                    Task { await checker.performPrimaryAction() }
                } label: {
                    PanelPill(
                        label: checker.isInstalling ? "Installing update…" : "Update available",
                        systemImage: "arrow.down.circle"
                    )
                }
                .buttonStyle(.plain)
                .disabled(!checker.canAct)
                .help(
                    checker.isInstalling
                        ? "Installing TokenMon \(release.version.description)"
                        : "Download, verify, and install TokenMon \(release.version.description)"
                )

                if let statusMessage = checker.statusMessage {
                    Text(statusMessage)
                        .font(PanelTypography.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, 10)
        }
    }
}
