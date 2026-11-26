import Foundation
import Observation

/// The state machine behind the sign-in sheet: when to capture the session,
/// what the header and the primary button say, and when the window closes.
///
/// The user signs in on the provider's page. Capture runs either when the
/// browser reports the return page or when the user presses Finish Sign-In;
/// a success shows a brief confirmation, then dismisses the window.
@MainActor
@Observable
final class ProviderSignInFlow {
    /// Where the sign-in stands.
    enum Phase: Equatable {
        case signingIn
        case finishing
        case signedIn(email: String?)
        case failed(String)
    }

    /// Suspends for the given number of nanoseconds; returns early when the
    /// task is cancelled.
    typealias Sleeper = @MainActor (UInt64) async -> Void

    /// How long the window waits before nudging the user to finish manually.
    static let timeoutHintNanoseconds: UInt64 = 25_000_000_000
    /// How long the "signed in" confirmation stays up before the window closes.
    static let confirmationNanoseconds: UInt64 = 1_100_000_000
    /// Wait before the one automatic retry when cookies arrive late.
    static let retryNanoseconds: UInt64 = 1_200_000_000
    /// Poll interval while an OAuth popup is still open, and how many polls.
    static let popupPollNanoseconds: UInt64 = 200_000_000
    static let popupPollLimit = 40

    private(set) var phase: Phase = .signingIn
    private(set) var didHintTimeout = false
    private(set) var isDismissed = false

    /// Chrome of the sheet's browser; its popup state shapes the header.
    let browser: SignInBrowserController
    @ObservationIgnored private let auth: any ProviderCookieCapturing
    @ObservationIgnored private let config: ProviderSignInConfig
    @ObservationIgnored private let onComplete: () -> Void
    @ObservationIgnored private let sleep: Sleeper
    /// Closes the window; the sheet installs its environment dismiss action.
    @ObservationIgnored var dismiss: () -> Void = {}

    init(
        auth: any ProviderCookieCapturing,
        config: ProviderSignInConfig,
        browser: SignInBrowserController? = nil,
        sleep: @escaping Sleeper = { try? await Task.sleep(nanoseconds: $0) },
        onComplete: @escaping () -> Void
    ) {
        self.auth = auth
        self.config = config
        self.browser = browser ?? SignInBrowserController()
        self.sleep = sleep
        self.onComplete = onComplete
    }

    // MARK: - Presentation

    /// The line under the title.
    var statusText: String {
        switch phase {
        case .signingIn:
            if browser.hasPopup {
                return "Complete sign-in in the popup above."
            }
            if didHintTimeout {
                return "Signed in? Choose Finish Sign-In to complete."
            }
            return config.subtitle
        case .finishing:
            return "Finishing sign-in…"
        case let .signedIn(email):
            return email.map { "Signed in as \($0)" } ?? "Signed in"
        case let .failed(message):
            return message
        }
    }

    /// The footer button's title.
    var primaryTitle: String {
        switch phase {
        case .signingIn: return "Finish Sign-In"
        case .finishing: return "Finishing…"
        case .failed: return "Try Again"
        case .signedIn: return "Signed In"
        }
    }

    var isBusy: Bool {
        phase == .finishing
    }

    var isSignedIn: Bool {
        if case .signedIn = phase { return true }
        return false
    }

    // MARK: - Lifecycle

    /// The window closed; pending waits stop short of capturing or dismissing.
    func markDismissed() {
        isDismissed = true
    }

    /// Waits `timeoutHintNanoseconds`, then nudges the user to finish manually.
    func waitForTimeoutHint() async {
        await sleep(Self.timeoutHintNanoseconds)
        if !Task.isCancelled { didHintTimeout = true }
    }

    // MARK: - Flow

    /// Manual finish: capture now and close, or surface a retry.
    func finish() async {
        guard !isDismissed, !isBusy, !isSignedIn else { return }
        phase = .finishing
        if await capture() {
            await confirmAndDismiss()
        } else {
            fail()
        }
    }

    /// Automatic capture after the user returns to the provider page.
    func captureAfterReturn() async {
        guard !isDismissed, !isBusy, !isSignedIn else { return }
        for _ in 0..<Self.popupPollLimit where browser.hasPopup {
            await sleep(Self.popupPollNanoseconds)
            if isDismissed { return }
        }
        if config.returnDelayNanoseconds > 0 {
            await sleep(config.returnDelayNanoseconds)
        }
        // The user may have finished manually during the wait.
        guard !isDismissed, !isBusy, !isSignedIn else { return }

        phase = .finishing
        if await capture() {
            await confirmAndDismiss()
            return
        }
        // Cookies may be late; retry once before asking the user.
        await sleep(Self.retryNanoseconds)
        guard !isDismissed, !isSignedIn else { return }
        if await capture() {
            await confirmAndDismiss()
        } else {
            fail()
        }
    }

    private func capture() async -> Bool {
        let ok = await auth.captureCookiesFromWebKit()
        if ok, let afterCapture = config.afterCapture {
            await afterCapture()
        }
        return ok
    }

    private func fail() {
        phase = .failed(auth.lastAuthError
            ?? "Couldn't find a signed-in session yet. Finish signing in, then try again.")
        browser.rearmReturn()
    }

    private func confirmAndDismiss() async {
        guard !isSignedIn else { return }
        phase = .signedIn(email: auth.accountEmail)
        onComplete()
        await sleep(Self.confirmationNanoseconds)
        if !isDismissed { dismiss() }
    }
}
