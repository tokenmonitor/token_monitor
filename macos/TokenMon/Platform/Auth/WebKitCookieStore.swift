import WebKit

/// Provider-neutral access to a WebKit cookie store.
/// Each provider session owns an isolated non-persistent store so each
/// provider's cookies stay in its own store (e.g. X/Twitter sessions stay out
/// of the Grok jar).
@MainActor
enum WKWebsiteDataStoreBridge {
    static let shared = WKWebsiteDataStoreBridgeImpl()
}

/// Main-actor bridge over `WKWebsiteDataStore` callback APIs.
@MainActor
final class WKWebsiteDataStoreBridgeImpl {
    /// All HTTP cookies currently in `store`.
    func allCookies(in store: WKWebsiteDataStore) async -> [HTTPCookie] {
        await withCheckedContinuation { continuation in
            store.httpCookieStore.getAllCookies { cookies in
                continuation.resume(returning: cookies)
            }
        }
    }

    /// Deletes cookies whose domain satisfies `predicate` from `store`.
    func clearCookies(matching predicate: (String) -> Bool, in store: WKWebsiteDataStore) async {
        let cookies = await allCookies(in: store)
        for cookie in cookies where predicate(cookie.domain) {
            await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                store.httpCookieStore.delete(cookie) {
                    continuation.resume()
                }
            }
        }
    }

    /// Removes every cookie from the given store (used for per-provider isolation).
    func clearAllCookies(in store: WKWebsiteDataStore) async {
        let cookies = await allCookies(in: store)
        for cookie in cookies {
            await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
                store.httpCookieStore.delete(cookie) {
                    continuation.resume()
                }
            }
        }
    }
}

/// The browser-wide cookie jars a provider sign-out also purges: WebKit's
/// default website data store and the process `HTTPCookieStorage`.
///
/// Production uses `live`, the user's real jars. The XCTest host injects
/// `isolated()`, so a test that signs a provider out never deletes the
/// developer's real browser sessions.
@MainActor
struct BrowserCookieJars {
    /// WebKit store whose provider-domain cookies are cleared on sign-out.
    let webDataStore: WKWebsiteDataStore
    /// Cookie storage whose provider-host cookies are cleared on sign-out.
    let httpCookieStorage: HTTPCookieStorage

    /// WebKit's default (persistent) store and `HTTPCookieStorage.shared`.
    static var live: BrowserCookieJars {
        BrowserCookieJars(webDataStore: .default(), httpCookieStorage: .shared)
    }

    /// Fresh in-memory jars: a non-persistent WebKit store and a private
    /// cookie storage, neither shared with another session or the system.
    static func isolated() -> BrowserCookieJars {
        BrowserCookieJars(webDataStore: .nonPersistent(), httpCookieStorage: InMemoryHTTPCookieStorage())
    }
}

/// An `HTTPCookieStorage` kept only in memory. A bare `HTTPCookieStorage()`
/// stores nothing, so this private jar overrides the accessors sign-out uses.
final class InMemoryHTTPCookieStorage: HTTPCookieStorage, @unchecked Sendable {
    private let lock = NSLock()
    private var stored: [HTTPCookie] = []

    override var cookies: [HTTPCookie]? {
        lock.withLock { stored }
    }

    override func setCookie(_ cookie: HTTPCookie) {
        lock.withLock {
            stored.removeAll { Self.isSame($0, cookie) }
            stored.append(cookie)
        }
    }

    override func deleteCookie(_ cookie: HTTPCookie) {
        lock.withLock { stored.removeAll { Self.isSame($0, cookie) } }
    }

    /// Cookies whose domain is `url`'s host or a parent domain of it.
    override func cookies(for url: URL) -> [HTTPCookie]? {
        guard let host = url.host?.lowercased() else { return [] }
        return lock.withLock {
            stored.filter { cookie in
                let domain = cookie.domain.lowercased().trimmingCharacters(in: CharacterSet(charactersIn: "."))
                return host == domain || host.hasSuffix(".\(domain)")
            }
        }
    }

    private static func isSame(_ lhs: HTTPCookie, _ rhs: HTTPCookie) -> Bool {
        lhs.name == rhs.name && lhs.domain == rhs.domain && lhs.path == rhs.path
    }
}
