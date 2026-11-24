@testable import TokenMon
import XCTest

/// Serves canned responses per URL path; no request leaves the process.
private final class GrokStubProtocol: URLProtocol {
    typealias Reply = (status: Int, headers: [String: String], body: Data)

    private static let lock = NSLock()
    private static var replies: [String: Reply] = [:]
    private static var paths: [String] = []

    static func reset(_ replies: [String: Reply]) {
        lock.lock()
        defer { lock.unlock() }
        self.replies = replies
        paths = []
    }

    static var requestedPaths: [String] {
        lock.lock()
        defer { lock.unlock() }
        return paths
    }

    override static func canInit(with request: URLRequest) -> Bool { true }
    override static func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let url = request.url else { return }
        Self.lock.lock()
        Self.paths.append(url.path)
        let reply = Self.replies[url.path] ?? (404, [:], Data())
        Self.lock.unlock()
        let response = HTTPURLResponse(url: url, statusCode: reply.status, httpVersion: nil, headerFields: reply.headers)!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: reply.body)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}

final class GrokUsageClientTests: XCTestCase {
    private let billingPath = GrokUsageClient.billingEndpoint.path
    private let grpcHex = "000000005f0a5d0d0000104212001a00220b08b1debfd20610b8efb07f2a0b08b1d3e4d20610b8efb07f" +
        "3a070804150000b8413a07080215000050413a020806421c0802120b08b1debfd20610b8efb07f1a0b08b1d3e4d20610b8efb07f" +
        "580162006801800000000f677270632d7374617475733a300d0a"

    private func client() -> GrokUsageClient {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [GrokStubProtocol.self]
        return GrokUsageClient(cookieHeader: "sso=x", accountEmail: nil, session: URLSession(configuration: configuration))
    }

    /// A fetch cancelled mid-request surfaces as `CancellationError`.
    func testCancelledFetchThrowsCancellationError() async {
        let client = GrokUsageClient(cookieHeader: "sso=x", accountEmail: nil, session: NeverRespondingURLProtocol.session())
        let task = Task { try await client.fetchUsage() }
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("expected cancellation")
        } catch {
            XCTAssertTrue(error is CancellationError, "got \(error)")
        }
    }

    override func tearDown() {
        GrokStubProtocol.reset([:])
        super.tearDown()
    }

    /// A refresh requests the billing endpoint and nothing else.
    func testBillingSuccessRequestsOnlyBilling() async throws {
        let body = try XCTUnwrap(Data(hexString: grpcHex))
        GrokStubProtocol.reset([billingPath: (200, ["Content-Type": "application/grpc-web+proto"], body)])

        let snapshot = try await client().fetchUsage()

        XCTAssertEqual(snapshot.usedPercent, 36, accuracy: 0.01)
        XCTAssertEqual(GrokStubProtocol.requestedPaths, [billingPath])
    }

    func testBillingUnauthorizedIsUnauthorized() async {
        GrokStubProtocol.reset([billingPath: (401, [:], Data())])

        do {
            _ = try await client().fetchUsage()
            XCTFail("expected an error")
        } catch {
            XCTAssertEqual((error as? ProviderError)?.usageError, .unauthorized)
        }
        XCTAssertEqual(GrokStubProtocol.requestedPaths, [billingPath])
    }

    /// A trailers-only UNAUTHENTICATED reply (status in the HTTP headers, empty
    /// body) is a rejected session, not an empty response.
    func testTrailersOnlyUnauthenticatedIsUnauthorized() async {
        GrokStubProtocol.reset([billingPath: (200, ["grpc-status": "16", "grpc-message": "unauthenticated"], Data())])

        do {
            _ = try await client().fetchUsage()
            XCTFail("expected an error")
        } catch {
            XCTAssertEqual((error as? ProviderError)?.usageError, .unauthorized)
        }
    }

    /// A Cloudflare 403 page on billing is transient.
    func testBillingChallengeIsTransient() async {
        let html = Data("<html><title>Just a moment...</title></html>".utf8)
        GrokStubProtocol.reset([billingPath: (403, ["Content-Type": "text/html"], html)])

        do {
            _ = try await client().fetchUsage()
            XCTFail("expected an error")
        } catch {
            XCTAssertNotEqual((error as? ProviderError)?.usageError, .unauthorized)
        }
    }

    /// A billing server error is reported without requesting any other path,
    /// and never counts against the session.
    func testBillingServerErrorRequestsNothingElse() async {
        GrokStubProtocol.reset([billingPath: (500, [:], Data())])

        do {
            _ = try await client().fetchUsage()
            XCTFail("expected an error")
        } catch {
            XCTAssertNotEqual((error as? ProviderError)?.usageError, .unauthorized)
        }
        XCTAssertEqual(GrokStubProtocol.requestedPaths, [billingPath])
    }

    // MARK: - Saved resets (GetRemainingResets)

    private let resetsPath = GrokUsageClient.remainingResetsEndpoint.path

    private static func varint(_ value: UInt64) -> [UInt8] {
        var value = value
        var bytes: [UInt8] = []
        repeat {
            let low = UInt8(value & 0x7F)
            value >>= 7
            bytes.append(value == 0 ? low : low | 0x80)
        } while value != 0
        return bytes
    }

    /// Length-delimited protobuf field.
    private static func field(_ number: UInt64, _ payload: [UInt8]) -> [UInt8] {
        varint(number << 3 | 2) + varint(UInt64(payload.count)) + payload
    }

    /// Timestamp message: field 1 = seconds.
    private static func timestamp(_ seconds: UInt64) -> [UInt8] {
        [0x08] + varint(seconds)
    }

    /// One reset token: id (10), granted_at (20), expires_at (30).
    private static func token(id: String, expires: UInt64) -> [UInt8] {
        field(10, Array(id.utf8)) + field(20, timestamp(1_780_000_000)) + field(30, timestamp(expires))
    }

    /// gRPC-web body: one data frame, then an OK trailer frame.
    private static func grpcWebBody(_ message: [UInt8], status: Int = 0) -> Data {
        let trailer = Array("grpc-status:\(status)\r\n".utf8)
        func frame(_ flags: UInt8, _ payload: [UInt8]) -> [UInt8] {
            let count = UInt32(payload.count)
            return [flags, UInt8(count >> 24), UInt8(count >> 16 & 0xFF), UInt8(count >> 8 & 0xFF), UInt8(count & 0xFF)]
                + payload
        }
        return Data(frame(0x00, message) + frame(0x80, trailer))
    }

    func testParseResetExpiriesReadsEachToken() throws {
        let body = Self.grpcWebBody(
            Self.field(10, Self.token(id: "tok_a", expires: 1_800_000_000))
                + Self.field(10, Self.token(id: "tok_b", expires: 1_790_000_000))
        )
        let expiries = try GRPCWebParser.parseResetExpiries(body)
        XCTAssertEqual(
            expiries.map(\.timeIntervalSince1970).sorted(),
            [1_790_000_000, 1_800_000_000]
        )
    }

    func testParseResetExpiriesEmptyReplyHasNoTokens() throws {
        XCTAssertEqual(try GRPCWebParser.parseResetExpiries(Self.grpcWebBody([])), [])
        XCTAssertEqual(try GRPCWebParser.parseResetExpiries(Data()), [])
    }

    /// A token whose expiry is not a plausible timestamp is not counted.
    func testParseResetExpiriesIgnoresImplausibleTimestamps() throws {
        let body = Self.grpcWebBody(Self.field(10, Self.token(id: "tok_a", expires: 42)))
        XCTAssertEqual(try GRPCWebParser.parseResetExpiries(body), [])
    }

    func testParseResetExpiriesThrowsOnTrailerError() {
        XCTAssertThrowsError(try GRPCWebParser.parseResetExpiries(Self.grpcWebBody([], status: 13)))
    }

    func testFetchSavedResetsCountsUnexpiredTokens() async throws {
        let now = Date(timeIntervalSince1970: 1_795_000_000)
        let body = Self.grpcWebBody(
            Self.field(10, Self.token(id: "tok_a", expires: 1_800_000_000))
                + Self.field(10, Self.token(id: "tok_old", expires: 1_790_000_000))
        )
        GrokStubProtocol.reset([resetsPath: (200, ["Content-Type": "application/grpc-web+proto"], body)])

        let resets = try await client().fetchSavedResets(now: now)

        XCTAssertEqual(resets?.count, 1)
        XCTAssertEqual(resets?.nextExpiry?.timeIntervalSince1970, 1_800_000_000)
        XCTAssertEqual(GrokStubProtocol.requestedPaths, [resetsPath])
    }

    /// A Cloudflare challenge on the resets path is an error the poller
    /// ignores, not a rejected session.
    func testFetchSavedResetsChallengeIsNotUnauthorized() async {
        let html = Data("<html><title>Just a moment...</title></html>".utf8)
        GrokStubProtocol.reset([resetsPath: (403, ["Content-Type": "text/html"], html)])

        do {
            _ = try await client().fetchSavedResets()
            XCTFail("expected an error")
        } catch {
            XCTAssertNotEqual((error as? ProviderError)?.usageError, .unauthorized)
        }
    }
}
