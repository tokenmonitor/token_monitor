import AppKit
import ServiceManagement
import SwiftUI
import UniformTypeIdentifiers

/// Settings window, in four tabs: General, Providers, Accounts, and Data.
///
/// Observes only the objects its own rows read (`settings`, `history`,
/// `updateChecker`); account and refresh rows are subviews that observe their
/// provider's session or poller.
struct PreferencesView: View {
    let model: AppModel
    @Bindable var settings: AppSettings
    let history: GrokHistoryStore
    let updateChecker: UpdateChecker
    let openWindow: (AppWindow) -> Void
    @State private var selectedTab: SettingsTab = .general

    var body: some View {
        TabView(selection: $selectedTab) {
            GeneralSettingsTab(model: model, settings: settings, updateChecker: updateChecker)
                .settingsTabItem(.general)
            ProvidersSettingsTab(settings: settings)
                .settingsTabItem(.providers)
            AccountsSettingsTab(model: model, openWindow: openWindow)
                .settingsTabItem(.accounts)
            DataSettingsTab(settings: settings, history: history)
                .settingsTabItem(.data)
        }
        .font(.system(size: 14))
        .padding()
        .frame(minWidth: 460, minHeight: 520)
        .onAppear {
            // Each opening of the window: with nothing signed in, start where
            // the user signs in.
            selectedTab = SettingsTab.initial(isAnyProviderSignedIn: model.isAnyProviderSignedIn)
            settings.refreshLaunchAtLoginStatus()
        }
        .onDisappear {
            AppDelegate.hideDockIfNoWindows()
        }
    }
}

private extension View {
    /// Labels this view as `tab` in the Settings `TabView` and tags it for selection.
    func settingsTabItem(_ tab: SettingsTab) -> some View {
        tabItem { Label(tab.displayName, systemImage: tab.systemImage) }
            .tag(tab)
    }
}

/// Startup, updates, refresh timing, alerts, and quitting.
struct GeneralSettingsTab: View {
    let model: AppModel
    @Bindable var settings: AppSettings
    let updateChecker: UpdateChecker
    var body: some View {
        Form {
            Section("Startup") {
                Toggle("Launch at Login", isOn: $settings.launchAtLogin)
                    .settingsSwitch()
                if settings.launchAtLoginNeedsApproval {
                    Text("Waiting for approval in System Settings › General › Login Items.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Button("Open Login Items…") { SMAppService.openSystemSettingsLoginItems() }
                }
            }

            Section {
                Toggle("Check automatically", isOn: $settings.checksForUpdates)
                    .settingsSwitch()
                LabeledContent("Version \(Self.appVersion)") {
                    Button(updateChecker.actionTitle) {
                        Task { await updateChecker.performPrimaryAction() }
                    }
                    .disabled(!updateChecker.canAct)
                }
                if let statusMessage = updateChecker.statusMessage {
                    Text(statusMessage)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            } header: {
                Text("Updates")
            } footer: {
                Text("Checks GitHub for a newer release. Update downloads the release and replaces this copy of TokenMon.")
            }

            Section("Refresh") {
                Stepper(value: $settings.activePollSeconds, in: AppSettings.activePollRange, step: 15) {
                    LabeledContent("While menu is open", value: "\(settings.activePollSeconds)s")
                }
                Stepper(value: $settings.idlePollSeconds, in: AppSettings.idlePollRange, step: 60) {
                    LabeledContent("While idle", value: "\(settings.idlePollSeconds)s")
                }
                refreshStatus
            }

            Section {
                Toggle("Notify when usage exceeds threshold", isOn: $settings.thresholdEnabled)
                    .settingsSwitch()
                if settings.thresholdEnabled {
                    Slider(value: $settings.thresholdPercent, in: AppSettings.thresholdRange, step: 1) {
                        Text("Alert at \(Int(settings.thresholdPercent))%")
                    } minimumValueLabel: {
                        Text("\(Int(AppSettings.thresholdRange.lowerBound))%")
                    } maximumValueLabel: {
                        Text("\(Int(AppSettings.thresholdRange.upperBound))%")
                    }
                    ForEach(settings.visibleUsageProviders) { provider in
                        Toggle(provider.displayName, isOn: Binding(
                            get: { settings.alertProviders.contains(provider) },
                            set: { on in
                                if on {
                                    settings.alertProviders.insert(provider)
                                } else {
                                    settings.alertProviders.remove(provider)
                                }
                            }
                        ))
                        .settingsSwitch()
                    }
                }
            } header: {
                Text("Alerts")
            } footer: {
                if settings.thresholdEnabled {
                    Text("Providers the alert watches keep refreshing in the background.")
                }
            }

            Section {
                Button("Quit TokenMon") {
                    NSApp.terminate(nil)
                }
            }
        }
        .formStyle(.grouped)
    }

    private static var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "unknown"
    }

    /// Last refresh (and error) of the selected provider, or every provider's
    /// last refresh on Overview.
    private var refreshStatus: some View {
        ForEach(refreshRows, id: \.provider) { row in
            pollerStatus(row.poller, label: row.label)
        }
    }

    /// One poller whose last refresh the Refresh section lists.
    struct RefreshRow {
        let provider: MonitorProvider
        let poller: any ProviderUsagePoller
        /// The provider name on Overview, which lists every provider; nil on
        /// a provider's own tab.
        let label: String?
    }

    /// Every provider on Overview, else the selected provider alone.
    var refreshRows: [RefreshRow] {
        let entries = model.providers.all
        if settings.selectedProvider == .overview {
            return entries.map { RefreshRow(provider: $0.provider, poller: $0.poller, label: $0.provider.displayName) }
        }
        return entries
            .filter { $0.provider == settings.selectedProvider }
            .map { RefreshRow(provider: $0.provider, poller: $0.poller, label: nil) }
    }

    private func pollerStatus(_ poller: some ProviderUsagePoller, label: String?) -> AnyView {
        AnyView(PollerRefreshStatus(poller: poller, label: label))
    }
}

/// Which provider tabs show and in what order, and what the menu bar draws.
struct ProvidersSettingsTab: View {
    @Bindable var settings: AppSettings
    @State private var draggingProvider: MonitorProvider?

    var body: some View {
        Form {
            Section {
                ForEach(settings.orderedUsageProviders) { provider in
                    providerRow(provider)
                }
                // Reordering uses the explicit drop delegate in `providerRow`,
                // since `onMove` is effectively inert on a macOS `Form`.
            } header: {
                Text("Providers")
            } footer: {
                Text("Turn a provider on to show its tab in the menu. Drag a row to set the order of tabs, Overview cards, and menu bar graphs.")
            }

            Section {
                Toggle("Show selected provider only", isOn: $settings.showSelectedProviderInMenuBar)
                    .settingsSwitch()
                if !settings.showSelectedProviderInMenuBar {
                    ForEach(MonitorProvider.menuBarGraphProviders) { provider in
                        Toggle("\(provider.displayName) bar graph", isOn: Binding(
                            get: { settings.showsMenuBarGraph(provider) },
                            set: { settings.setMenuBarGraph(provider, shown: $0) }
                        ))
                        .settingsSwitch()
                        if provider == .grok {
                            Toggle("Grok categories", isOn: $settings.showCategoriesInMenuBar)
                                .settingsSwitch()
                        }
                    }
                }
            } header: {
                Text("Menu Bar")
            } footer: {
                Text(settings.showSelectedProviderInMenuBar
                    ? "The menu bar shows only the active provider's icon, percentage, and usage bar."
                    : "Choose which graphs stay pinned in the menu bar.")
            }
        }
        .formStyle(.grouped)
    }

    private func providerRow(_ provider: MonitorProvider) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "line.3.horizontal")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(width: 20, height: 24)
                .contentShape(Rectangle())
                .onDrag {
                    draggingProvider = provider
                    return NSItemProvider(object: NSString(string: provider.rawValue))
                }
            Image(nsImage: ProviderLogo.image(for: provider))
                .resizable()
                .interpolation(.high)
                .scaledToFit()
                .frame(width: 16, height: 16)
            Toggle(provider.displayName, isOn: Binding(
                get: { settings.enabledProviderIDs.contains(provider) },
                set: { on in
                    if on {
                        settings.enabledProviderIDs.insert(provider)
                    } else {
                        settings.enabledProviderIDs.remove(provider)
                    }
                }
            ))
            .settingsSwitch()
            .disabled(settings.enabledProviderIDs == [provider])
        }
        .contentShape(Rectangle())
        .onDrop(
            of: [.utf8PlainText, .text, .plainText],
            delegate: ProviderReorderDropDelegate(
                target: provider,
                dragging: $draggingProvider,
                move: settings.moveProvider
            )
        )
    }
}

/// SuperGrok categories plus exporting and clearing local history.
struct DataSettingsTab: View {
    /// An exported history file: its bytes, suggested name and type.
    struct ExportFile {
        let data: Data
        let filename: String
        let contentType: UTType
    }

    @Bindable var settings: AppSettings
    let history: GrokHistoryStore
    /// Saves an export; the default asks where with a save panel.
    let save: (ExportFile) throws -> Void
    @State private var exportError: String?

    /// - Parameter exportError: The export failure shown when the tab opens.
    init(
        settings: AppSettings,
        history: GrokHistoryStore,
        exportError: String? = nil,
        save: @escaping (ExportFile) throws -> Void = DataSettingsTab.saveWithPanel
    ) {
        self.settings = settings
        self.history = history
        self.save = save
        _exportError = State(initialValue: exportError)
    }

    var body: some View {
        Form {
            Section {
                ForEach(ProductCatalog.knownIDs, id: \.self) { id in
                    Toggle(ProductCatalog.displayName(for: id), isOn: Binding(
                        get: { settings.visibleProductIDs.contains(id) },
                        set: { on in
                            if on {
                                settings.visibleProductIDs.insert(id)
                            } else {
                                settings.visibleProductIDs.remove(id)
                            }
                        }
                    ))
                    .settingsSwitch()
                }
            } header: {
                Text("SuperGrok Categories")
            } footer: {
                Text("Categories shown in the Grok breakdown.")
            }

            Section {
                LabeledContent("Export history") {
                    HStack {
                        Button("CSV…") { export(.csv) }
                        Button("JSON…") { export(.json) }
                    }
                }
                Button("Clear Local History", role: .destructive) {
                    history.clear()
                }
                if let exportError {
                    Text(exportError)
                        .font(.caption)
                        .foregroundStyle(.red)
                }
                if history.storeFailed {
                    Text("History storage is unavailable — usage is kept for this session only "
                        + "and will not survive a relaunch.")
                        .font(.caption)
                        .foregroundStyle(.red)
                }
            } header: {
                Text("SuperGrok History")
            } footer: {
                Text("Clearing history does not reset your SuperGrok weekly pool.")
            }
        }
        .formStyle(.grouped)
    }

    private func export(_ format: GrokExportService.Format) {
        // Clears any prior failure so a successful export dismisses the red
        // message.
        exportError = nil
        do {
            try save(Self.exportFile(history.allSnapshots(), format: format))
        } catch {
            exportError = error.localizedDescription
        }
    }

    /// The history encoded as `format`, with its file name and type.
    static func exportFile(_ snapshots: [WeeklyUsageSnapshot], format: GrokExportService.Format) throws -> ExportFile {
        let data = try GrokExportService.export(snapshots, format: format)
        switch format {
        case .csv: return ExportFile(data: data, filename: "grok-usage.csv", contentType: .commaSeparatedText)
        case .json: return ExportFile(data: data, filename: "grok-usage.json", contentType: .json)
        }
    }

    /// Asks where to save `file`, then writes it.
    static func saveWithPanel(_ file: ExportFile) throws {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [file.contentType]
        panel.nameFieldStringValue = file.filename
        if panel.runModal() == .OK, let url = panel.url {
            try file.data.write(to: url)
        }
    }
}

/// Form rows ignore `onMove` on macOS; this delegate reorders while dragging
/// a handle over another provider row.
struct ProviderReorderDropDelegate: DropDelegate {
    let target: MonitorProvider
    @Binding var dragging: MonitorProvider?
    let move: (MonitorProvider, MonitorProvider) -> Void

    func validateDrop(info: DropInfo) -> Bool { true }

    func dropEntered(info: DropInfo) {
        withAnimation(.easeInOut(duration: 0.12)) {
            moveDraggedProvider()
        }
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func performDrop(info: DropInfo) -> Bool {
        finishDrag()
    }

    /// Moves the dragged provider to this row's place; a row dragged over
    /// itself stays put.
    func moveDraggedProvider() {
        guard let dragging, dragging != target else { return }
        move(dragging, target)
    }

    /// Ends the drag; the drop is always accepted.
    func finishDrag() -> Bool {
        dragging = nil
        return true
    }
}

/// A poller's last refresh time and, without a `label`, its last error.
struct PollerRefreshStatus<Poller: ProviderUsagePoller>: View {
    let poller: Poller
    /// Prefixes the time with the provider name (Overview lists every provider).
    let label: String?

    var body: some View {
        if let last = poller.lastRefreshedAt {
            let time = last.formatted(date: .abbreviated, time: .shortened)
            Text(label.map { "\($0): \(time)" } ?? "Last refresh: \(time)")
                .foregroundStyle(.secondary)
        }
        if label == nil, let error = poller.lastError {
            Text(error)
                .foregroundStyle(.red)
                .font(.caption)
        }
    }
}

private extension View {
    /// A switch at the compact mini size macOS System Settings uses.
    func settingsSwitch() -> some View {
        toggleStyle(.switch).controlSize(.mini)
    }
}
