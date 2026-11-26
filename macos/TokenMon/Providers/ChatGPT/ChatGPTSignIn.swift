import Foundation

extension ChatGPTAuthSession: ProviderSignInProviding {
    /// Identity-provider hosts of the ChatGPT sign-in, matched exactly or as a
    /// dot-suffix subdomain (never by substring: `auth.openai.com.evil.example`
    /// is not OpenAI).
    nonisolated static let authIdPHosts = [
        "auth.openai.com",
        "auth0.openai.com",
        "accounts.google.com",
        "appleid.apple.com",
        "idmsa.apple.com",
        "login.microsoftonline.com",
        "login.microsoft.com",
        "login.live.com"
    ]

    /// True for an OpenAI/Google/Apple/Microsoft identity-provider host, or a
    /// `/login` or `/signin` path on chatgpt.com itself.
    nonisolated static func isAuthHost(host: String, path: String) -> Bool {
        let path = path.lowercased()
        return Domain.matches(host, hosts: authIdPHosts)
            || (Domain.matches(host, hosts: ["chatgpt.com"])
                && (path.contains("/login") || path.contains("/signin")))
    }

    /// True on chatgpt.com after login — excludes login/signin/auth paths.
    nonisolated static func isReturnPage(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return false }
        guard Domain.matches(host, hosts: ["chatgpt.com"]) else { return false }
        let path = url.path.lowercased()
        if path.contains("login") || path.contains("signin") || path.contains("/auth") {
            return false
        }
        return true
    }

    /// Captures the NextAuth session cookie.
    var signInConfig: ProviderSignInConfig {
        ProviderSignInConfig(
            title: "Sign in to ChatGPT",
            subtitle: "Sign in to your ChatGPT account. This window finishes on its own once you return to chatgpt.com.",
            startURL: URL(staticString: "https://chatgpt.com/"),
            isAuthHost: Self.isAuthHost,
            isReturnPage: Self.isReturnPage
        )
    }
}
