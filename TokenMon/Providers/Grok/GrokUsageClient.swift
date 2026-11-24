import Foundation
import os

extension ProviderError {
    private static let logger = Logger(category: "GrokUsageClient")

    /// Body-free error: the raw response body is logged privately (truncated) for
    /// diagnostics, and the user-facing message, which is logged at
    /// `privacy: .public` and rendered in the panel, carries only the status code.
    static func grokHTTPStatus(_ code: Int, body: String) -> ProviderError {
        if !body.isEmpty {
            logger.debug("Grok HTTP \(code) body: \(String(body.prefix(400)), privacy: .private)")
        }
        let message = "Usage request failed (HTTP \(code))."
        return .custom(message: message, usage: .badResponse(message))
    }

    /// Maps a usage-response decoding failure to a user-facing `ProviderError`.
    static func grokDecoding(_ detail: String) -> ProviderError {
        let message = "Could not parse usage response: \(detail)"
        return .custom(message: message, usage: .badResponse(message))
    }

    static var grokEmptyResponse: ProviderError {
        let message = "Empty usage response from Grok."
        return .custom(message: message, usage: .badResponse(message))
    }
}

/// Fetches SuperGrok weekly usage from the gRPC-web billing endpoint behind
/// grok.com Settings → Usage.
struct GrokUsageClient: Sendable {
    private let logger = Logger(category: "GrokUsageClient")

    /// gRPC-web billing endpoint used by grok.com Settings → Usage.
    static let billingEndpoint = URL(staticString: "https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig")
    /// gRPC-web endpoint listing saved usage-limit reset coupons.
    static let remainingResetsEndpoint = URL(staticString: "https://grok.com/prod_mc_billing.ConsumerUiSvc/GetRemainingResets")

    var cookieHeader: String?
    var accountEmail: String?
    var session: URLSession = ProviderURLSession.shared

    /// Fetches usage from the gRPC-web billing endpoint.
    ///
    /// - Throws: `.notSignedIn` without a cookie header, `.unauthorized` when
    ///   the endpoint rejects the session, otherwise the request or parse error.
    func fetchUsage() async throws -> WeeklyUsageSnapshot {
        if cookieHeader == nil {
            throw ProviderError.notSignedIn(.grok)
        }
        return try await fetchGRPCWebBilling()
    }

    /// Saved reset coupons that have not expired at `now`, or `nil` when
    /// there are none.
    ///
    /// - Throws: `.notSignedIn` without a cookie header, otherwise any
    ///   request, status or parse error. Callers treat every error as "no
    ///   new information": this call never decides whether the session is valid.
    func fetchSavedResets(now: Date = Date()) async throws -> SavedResets? {
        if cookieHeader == nil {
            throw ProviderError.notSignedIn(.grok)
        }
        let data = try await postGRPCWeb(to: Self.remainingResetsEndpoint)
        return try SavedResets(expirations: GRPCWebParser.parseResetExpiries(data), now: now)
    }

    // MARK: - gRPC-web

    private func fetchGRPCWebBilling() async throws -> WeeklyUsageSnapshot {
        let data = try await postGRPCWeb(to: Self.billingEndpoint)
        try GRPCWebParser.validateTrailers(data)
        let parsed = try GRPCWebParser.parseUsage(data)
        let used = parsed.usedPercent
        let products = parsed.products.isEmpty
            ? Self.synthesizeProducts(usedPercent: used)
            : parsed.products
        let productLog = products.map { "\($0.id):\(String(format: "%.1f", $0.percentOfPool))%" }.joined(separator: ", ")
        logger.info("gRPC parsed products: \(productLog, privacy: .public)")
        #if DEBUG
        logger.debug("gRPC field dump:\n\(GRPCWebParser.debugFieldDump(data), privacy: .public)")
        #endif
        return WeeklyUsageSnapshot(
            usedPercent: used,
            remainingPercent: max(0, 100 - used),
            resetsAt: parsed.resetsAt,
            products: products,
            accountEmail: accountEmail
        )
    }

    /// POSTs an empty gRPC-web message to `url` and returns the body once the
    /// HTTP status and any header `grpc-status` are clean. Trailer-frame status
    /// is left to the parser.
    private func postGRPCWeb(to url: URL) async throws -> Data {
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = 15
        // Empty gRPC-web frame (5-byte header + empty message).
        request.httpBody = Data([0x00, 0x00, 0x00, 0x00, 0x00])
        applyAuth(to: &request)
        request.setValue("application/grpc-web+proto", forHTTPHeaderField: "Content-Type")
        request.setValue("1", forHTTPHeaderField: "x-grpc-web")
        request.setValue("connect-es/2.1.1", forHTTPHeaderField: "x-user-agent")
        request.setValue("*/*", forHTTPHeaderField: "Accept")
        request.setValue("https://grok.com", forHTTPHeaderField: "Origin")
        request.setValue("https://grok.com/?_s=usage", forHTTPHeaderField: "Referer")
        request.setValue(AppIdentity.userAgent, forHTTPHeaderField: "User-Agent")

        let (data, response) = try await AuthenticatedRequest.data(for: request, session: session)
        guard let http = response as? HTTPURLResponse else {
            throw ProviderError.network(.grok, "Invalid response")
        }
        if ProviderHTTP.isBotChallenge(http, data: data) {
            let message = ProviderHTTP.botChallengeMessage(status: http.statusCode)
            throw ProviderError.custom(message: message, usage: .badResponse(message))
        }
        if http.statusCode == 401 || http.statusCode == 403 {
            throw ProviderError.unauthorized(.grok)
        }
        guard http.statusCode == 200 else {
            let body = String(data: data.prefix(400), encoding: .utf8) ?? ""
            throw ProviderError.grokHTTPStatus(http.statusCode, body: body)
        }

        // A trailers-only reply carries `grpc-status` in the HTTP headers.
        try GRPCWebParser.validateHeaders(http)
        return data
    }

    private func applyAuth(to request: inout URLRequest) {
        if let cookieHeader, !cookieHeader.isEmpty {
            request.setValue(cookieHeader, forHTTPHeaderField: "Cookie")
        }
    }

    /// Without a product breakdown, attributes the pool to a neutral "other" slice.
    static func synthesizeProducts(usedPercent: Double) -> [ProductUsage] {
        guard usedPercent > 0.05 else { return [] }
        return [
            ProductUsage(
                id: "other",
                displayName: "Other",
                percentOfPool: usedPercent,
                colorToken: .other
            )
        ]
    }
}
