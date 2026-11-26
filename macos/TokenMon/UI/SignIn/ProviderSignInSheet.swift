import AppKit
import SwiftUI
import WebKit

/// A session that signs in through the shared sign-in sheet.
@MainActor
protocol ProviderSignInProviding: ProviderCookieCapturing {
    /// The provider's sign-in page and how the sheet recognises its steps.
    var signInConfig: ProviderSignInConfig { get }
}

/// Auth types that can capture a WebKit session for a provider sign-in sheet.
@MainActor
protocol ProviderCookieCapturing: AnyObject, Observable {
    var lastAuthError: String? { get }
    var accountEmail: String? { get }
    /// Isolated WebKit store this provider signs into and captures from.
    var signInDataStore: WKWebsiteDataStore { get }
    func captureCookiesFromWebKit() async -> Bool
}

/// Per-provider configuration for the shared sign-in sheet.
struct ProviderSignInConfig {
    var title: String
    var subtitle: String
    var startURL: URL
    var isAuthHost: (String, String) -> Bool
    var isReturnPage: (URL) -> Bool
    var returnDelayNanoseconds: UInt64
    /// Runs when the return page is detected, before the delayed capture.
    var onReturned: ((URL) -> Void)?
    /// Runs after a successful cookie capture, before dismissal.
    var afterCapture: (() async -> Void)?

    init(
        title: String,
        subtitle: String,
        startURL: URL,
        isAuthHost: @escaping (String, String) -> Bool,
        isReturnPage: @escaping (URL) -> Bool,
        returnDelayNanoseconds: UInt64 = 800_000_000,
        onReturned: ((URL) -> Void)? = nil,
        afterCapture: (() async -> Void)? = nil
    ) {
        self.title = title
        self.subtitle = subtitle
        self.startURL = startURL
        self.isAuthHost = isAuthHost
        self.isReturnPage = isReturnPage
        self.returnDelayNanoseconds = returnDelayNanoseconds
        self.onReturned = onReturned
        self.afterCapture = afterCapture
    }
}

/// Guided sign-in: the user signs in on the provider's own page and the sheet
/// finishes on its own, showing a brief confirmation before it closes.
struct ProviderSignInSheet<Auth: ProviderCookieCapturing>: View {
    let auth: Auth
    let config: ProviderSignInConfig
    var onComplete: () -> Void

    @State private var flow: ProviderSignInFlow
    @Environment(\.dismiss) private var dismiss

    /// - Parameter flow: The sign-in state machine; built from `auth` and
    ///   `config` when nil.
    init(
        auth: Auth,
        config: ProviderSignInConfig,
        flow: ProviderSignInFlow? = nil,
        onComplete: @escaping () -> Void
    ) {
        self.auth = auth
        self.config = config
        self.onComplete = onComplete
        _flow = State(initialValue: flow ?? ProviderSignInFlow(auth: auth, config: config, onComplete: onComplete))
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            SignInNavigationBar(browser: flow.browser)
            Divider()

            ProviderSignInWebView(
                startURL: config.startURL,
                dataStore: auth.signInDataStore,
                controller: flow.browser,
                isAuthHost: config.isAuthHost,
                isReturnPage: config.isReturnPage,
                onAuthHostSeen: {},
                onReturned: returned
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            Divider()
            footer
        }
        .frame(minWidth: 880, minHeight: 640)
        .onAppear {
            let dismiss = dismiss
            flow.dismiss = { dismiss() }
            NSApp.activate()
        }
        .onDisappear { flow.markDismissed() }
        .task { await flow.waitForTimeoutHint() }
    }

    /// The browser reached the provider's return page after signing in.
    func returned(_ url: URL) {
        config.onReturned?(url)
        Task { await flow.captureAfterReturn() }
    }

    // MARK: - Header

    private var header: some View {
        HStack(alignment: .top, spacing: 12) {
            if flow.isSignedIn {
                Image(systemName: "checkmark.circle.fill")
                    .font(.title2)
                    .foregroundStyle(.green)
            }

            VStack(alignment: .leading, spacing: 2) {
                Text(config.title)
                    .font(.title2.weight(.semibold))
                Text(flow.statusText)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer()

            Button("Cancel") { dismiss() }
                .keyboardShortcut(.cancelAction)
        }
        .padding()
    }

    // MARK: - Footer

    private var footer: some View {
        HStack(spacing: 12) {
            if flow.isBusy {
                ProgressView().controlSize(.small)
            }

            Spacer()

            Button(flow.primaryTitle) {
                Task { await flow.finish() }
            }
            .keyboardShortcut(.defaultAction)
            .disabled(flow.isBusy || flow.isSignedIn)
        }
        .padding()
    }
}

/// Back / Forward / Reload / Close popup chrome for the sign-in WebKit host.
struct SignInNavigationBar: View {
    let browser: SignInBrowserController
    var body: some View {
        HStack(spacing: 8) {
            Button(action: browser.goBack) {
                Image(systemName: "chevron.backward")
            }
            .disabled(!browser.backIsEnabled)
            .help(browser.hasPopup && !browser.canGoBack ? "Back to the sign-in page" : "Back")
            .accessibilityLabel("Back")

            Button(action: browser.goForward) {
                Image(systemName: "chevron.forward")
            }
            .disabled(!browser.canGoForward)
            .help("Forward")
            .accessibilityLabel("Forward")

            Button(action: browser.reload) {
                Image(systemName: browser.isLoading ? "xmark" : "arrow.clockwise")
            }
            .help(browser.isLoading ? "Stop" : "Reload")
            .accessibilityLabel(browser.isLoading ? "Stop" : "Reload")

            Text(browser.hasPopup ? "Sign-in popup" : displayURL)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: .infinity, alignment: .leading)
                .help(browser.currentURL)

            if browser.hasPopup {
                Button("Close popup", action: browser.closePopup)
                    .help("Return to the provider sign-in page")
            }

            if browser.isLoading {
                ProgressView()
                    .controlSize(.small)
            }
        }
        .buttonStyle(.borderless)
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    private var displayURL: String {
        guard let url = URL(string: browser.currentURL), let host = url.host else {
            return browser.currentURL
        }
        return host + url.path
    }
}
