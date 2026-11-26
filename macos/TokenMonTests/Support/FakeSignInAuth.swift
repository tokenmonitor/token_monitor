import Observation
@testable import TokenMon
import WebKit

/// A sign-in session whose captures return scripted results.
@MainActor
@Observable
final class FakeSignInAuth: ProviderCookieCapturing {
    var lastAuthError: String?
    var accountEmail: String?
    let signInDataStore = WKWebsiteDataStore.nonPersistent()
    /// Results of the next captures, in order; an empty queue fails.
    var captureResults: [Bool] = []
    private(set) var captureCount = 0
    /// Runs inside each capture, before it returns.
    var duringCapture: (() -> Void)?

    init(captureResults: [Bool] = [], accountEmail: String? = nil, lastAuthError: String? = nil) {
        self.captureResults = captureResults
        self.accountEmail = accountEmail
        self.lastAuthError = lastAuthError
    }

    func captureCookiesFromWebKit() async -> Bool {
        captureCount += 1
        duringCapture?()
        return captureResults.isEmpty ? false : captureResults.removeFirst()
    }
}

extension ProviderSignInConfig {
    /// A sign-in page that never loads anything from the network.
    static func fixture(
        returnDelayNanoseconds: UInt64 = 0,
        onReturned: ((URL) -> Void)? = nil,
        afterCapture: (() async -> Void)? = nil
    ) -> ProviderSignInConfig {
        ProviderSignInConfig(
            title: "Sign in to Example",
            subtitle: "Sign in with your Example account.",
            startURL: URL(string: "about:blank")!,
            isAuthHost: { host, _ in host == "auth.example.com" },
            isReturnPage: { $0.host == "example.com" },
            returnDelayNanoseconds: returnDelayNanoseconds,
            onReturned: onReturned,
            afterCapture: afterCapture
        )
    }
}
