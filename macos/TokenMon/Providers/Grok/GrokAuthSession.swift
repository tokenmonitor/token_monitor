import Foundation

private let grokHosts = ["grok.com", "x.ai"]

/// Session for grok.com (`sso` / `sso-rw` cookies).
///
/// Sign-in may pass through x.com / twitter.com (see `GrokAuthSession.authHosts`);
/// the session captures and stores only the grok.com/xAI cookies.
@MainActor
final class GrokAuthSession: ProviderAuthSession {
    /// The grok.com/xAI session cookies the usage requests send. Capture stores
    /// only these, and waits until one of them is present.
    nonisolated static let essentialCookieNames: Set<String> = ["sso", "sso-rw"]

    convenience init() {
        self.init(directory: nil)
    }

    /// Test seam: isolates the session's file store to `directory`, or uses `store`.
    init(directory: URL?, store: (any CredentialStore)? = nil) {
        super.init(
            config: ProviderAuthConfig(
                storeFilenamePrefix: "auth_",
                logCategory: "Auth",
                extraStoreKeys: [],
                signOutHosts: grokHosts,
                capturePolicy: Self.grokPolicy(),
                isDomain: { domain in Self.isGrokDomain(domain) }
            ),
            directory: directory,
            store: store
        )
    }

    /// Cookie-capture policy for the grok.com/xAI sign-in flow.
    static func grokPolicy() -> WebKitCookieCapture.Policy {
        WebKitCookieCapture.Policy(
            isDomain: { domain in Self.isGrokDomain(domain) },
            isPreferredSessionCookie: { cookie in
                Self.essentialCookieNames.contains(cookie.name.lowercased())
            },
            includeAllDomainCookiesWhenSessionFound: true,
            essentialCookieNames: Self.essentialCookieNames,
            maxAttempts: 4,
            failureMessage: "No session cookies found. Finish signing in, then click Capture Session."
        )
    }

    nonisolated static func isGrokDomain(_ domain: String) -> Bool {
        Domain.matches(domain, hosts: grokHosts)
    }
}
