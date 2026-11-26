import SwiftUI

/// One row per provider account: who is signed in, and sign-in actions.
struct AccountsSettingsTab: View {
    let model: AppModel
    let openWindow: (AppWindow) -> Void

    var body: some View {
        Form {
            Section {
                Label {
                    Text(AccountsPrivacyNote.text)
                        .fixedSize(horizontal: false, vertical: true)
                } icon: {
                    Image(systemName: "lock.shield")
                }
                .foregroundStyle(.secondary)
            }

            Section {
                ProviderAccountRow(
                    provider: .grok,
                    auth: model.grokAuth,
                    signInTitle: "Sign In…",
                    openSignIn: { openWindow(.signIn(.grok)) }
                )
                ProviderAccountRow(
                    provider: .claude,
                    auth: model.claudeAuth,
                    signInTitle: "Sign In…",
                    openSignIn: { openWindow(.signIn(.claude)) }
                )
                ProviderAccountRow(
                    provider: .chatgpt,
                    auth: model.chatGPTAuth,
                    signInTitle: "Sign In…",
                    openSignIn: { openWindow(.signIn(.chatgpt)) }
                )
                ProviderAccountRow(
                    provider: .cursor,
                    auth: model.cursorAuth,
                    signInTitle: "Sign In…",
                    openSignIn: { openWindow(.signIn(.cursor)) }
                )
                ProviderAccountRow(
                    provider: .opencode,
                    auth: model.openCodeAuth,
                    signInTitle: "Sign In…",
                    openSignIn: { openWindow(.signIn(.opencode)) }
                )
            } header: {
                Text("Accounts")
            } footer: {
                Text("Grokbot uses your Cursor account.")
            }

            OpenRouterAccountSection(auth: model.openRouterAuth, poller: model.openRouterPoller)
        }
        .formStyle(.grouped)
    }
}

/// One cookie-session provider's account on a single row: its logo and name,
/// who is signed in, and the sign-in actions.
struct ProviderAccountRow: View {
    let provider: MonitorProvider
    let auth: ProviderAuthSession
    let signInTitle: String
    let openSignIn: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 10) {
                Image(nsImage: ProviderLogo.image(for: provider))
                    .resizable()
                    .interpolation(.high)
                    .scaledToFit()
                    .frame(width: 16, height: 16)
                VStack(alignment: .leading, spacing: 2) {
                    Text(provider.displayName)
                    Text(auth.isSignedIn ? (auth.accountEmail ?? "Signed in") : "Not signed in")
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer(minLength: 8)
                if auth.isSignedIn {
                    Button("Re-authenticate…") { openSignIn() }
                    Button("Sign Out", role: .destructive) {
                        auth.signOut()
                    }
                } else {
                    Button(signInTitle) { openSignIn() }
                }
            }
            if let err = auth.lastAuthError {
                Text(err).foregroundStyle(.red).font(.caption)
            }
        }
    }
}

/// OpenRouter account rows; OpenRouter connects with an API key.
struct OpenRouterAccountSection: View {
    let auth: OpenRouterAuthSession
    let poller: OpenRouterUsagePoller
    @State private var keyDraft: String

    /// - Parameter keyDraft: The key typed when the section opens.
    init(auth: OpenRouterAuthSession, poller: OpenRouterUsagePoller, keyDraft: String = "") {
        self.auth = auth
        self.poller = poller
        _keyDraft = State(initialValue: keyDraft)
    }

    var body: some View {
        Section("OpenRouter") {
            if auth.isSignedIn {
                LabeledContent("Connected with") {
                    Text("OpenRouter API key")
                }
                Button("Sign Out", role: .destructive) {
                    auth.signOut()
                    keyDraft = ""
                }
            } else {
                Text("Not connected")
                    .foregroundStyle(.secondary)
                SecureField("sk-or-v1-…", text: $keyDraft)
                Button("Save API Key") { saveKey() }
                    .disabled(keyDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                if let err = auth.lastAuthError {
                    Text(err).foregroundStyle(.red).font(.caption)
                }
            }
        }
    }

    private func saveKey() {
        if auth.saveAPIKey(keyDraft) {
            keyDraft = ""
            Task { await poller.refreshNow() }
        }
    }
}
