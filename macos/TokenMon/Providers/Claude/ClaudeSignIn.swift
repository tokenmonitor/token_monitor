import Foundation

extension ClaudeAuthSession: ProviderSignInProviding {
    /// Identity-provider hosts of the Claude sign-in, matched exactly or as a
    /// dot-suffix subdomain (never by substring: `github.com.evil.example` is
    /// not GitHub).
    nonisolated static let authIdPHosts = [
        "clerk.claude.ai",
        "accounts.google.com",
        "github.com",
        "appleid.apple.com",
        "idmsa.apple.com"
    ]

    /// True for a Claude identity-provider host, or a `/login` or `/signin`
    /// path on claude.ai itself (a look-alike host with a login path is not
    /// an auth page).
    nonisolated static func isAuthHost(host: String, path: String) -> Bool {
        let path = path.lowercased()
        return Domain.matches(host, hosts: authIdPHosts)
            || (Domain.matches(host, hosts: ["claude.ai"])
                && (path.contains("/login") || path.contains("/signin")))
    }

    /// True on claude.ai after login. Excludes the Clerk SSO subdomain
    /// (`clerk.claude.ai`) and login/signin/auth paths, so capture fires only
    /// on a signed-in page.
    nonisolated static func isReturnPage(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased(), host == "claude.ai" || host == "www.claude.ai" else { return false }
        let path = url.path.lowercased()
        if path.contains("login") || path.contains("signin") || path.contains("/auth") {
            return false
        }
        return true
    }

    /// Captures the `sessionKey` session cookie.
    var signInConfig: ProviderSignInConfig {
        ProviderSignInConfig(
            title: "Sign in to Claude",
            subtitle: "Sign in to your Claude account. This window finishes on its own once you return to claude.ai.",
            startURL: URL(staticString: "https://claude.ai/new"),
            isAuthHost: Self.isAuthHost,
            isReturnPage: Self.isReturnPage
        )
    }
}
