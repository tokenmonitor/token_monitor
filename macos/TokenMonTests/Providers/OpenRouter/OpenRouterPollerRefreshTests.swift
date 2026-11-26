@testable import TokenMon
import XCTest

/// The OpenRouter poller's refresh and the live client's requests: the key
/// gate, what one poll publishes, the poll interval, and the bearer-key
/// requests (with `/credits` and `/activity` only for a management key).
@MainActor
final class OpenRouterPollerRefreshTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var settings: AppSettings!
    private var auth: OpenRouterAuthSession!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "OpenRouterPollerRefresh-\(UUID().uuidString)"
        settings = AppSettings(defaults: try XCTUnwrap(UserDefaults(suiteName: suiteName)))
        settings.selectedProvider = .openrouter
        auth = OpenRouterAuthSession(directory: dir)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        UserDefaults(suiteName: suiteName)?.removePersistentDomain(forName: suiteName)
    }

    private func keySnapshot(usage: Double, limit: Double?) async throws -> OpenRouterSnapshot {
        let limitJSON = limit.map { String($0) } ?? "null"
        let client = OpenRouterUsageClient { _ in
            Data(#"{"data":{"usage":\#(usage),"limit":\#(limitJSON),"is_management_key":false}}"#.utf8)
        }
        return try await client.fetchSnapshot()
    }

    func testWithoutAKeyAsksForOne() async {
        let poller = OpenRouterUsagePoller(settings: settings, auth: auth) { _ in
            XCTFail("no fetch without a key")
            throw CancellationError()
        }
        await poller.refreshNow()
        XCTAssertTrue(auth.needsSignIn)
        XCTAssertEqual(poller.lastError, "Add an OpenRouter API key to load usage.")
    }

    func testRejectedKeyStopsPollingUntilANewOneIsSaved() async {
        auth.saveAPIKey("sk-or-v1-old")
        auth.markSessionInvalid(reason: "rejected")
        var fetches = 0
        let poller = OpenRouterUsagePoller(settings: settings, auth: auth) { _ in
            fetches += 1
            throw CancellationError()
        }
        await poller.refreshNow()
        XCTAssertEqual(fetches, 0)
    }

    func testRefreshPublishesTheKeySpend() async throws {
        auth.saveAPIKey("sk-or-v1-live")
        let capped = try await keySnapshot(usage: 5, limit: 20)
        let uncapped = try await keySnapshot(usage: 5, limit: nil)
        var replies = [capped, uncapped]
        var sentKey: String?
        let poller = OpenRouterUsagePoller(settings: settings, auth: auth) { key in
            sentKey = key
            return replies.removeFirst()
        }

        await poller.refreshNow()
        XCTAssertEqual(sentKey, "sk-or-v1-live")
        XCTAssertEqual(poller.headlineUsedPercent ?? -1, 25, accuracy: 0.001)
        XCTAssertNil(poller.headlineResetsAt)
        XCTAssertNil(poller.alertAccount)
        XCTAssertNotNil(poller.lastRefreshedAt)

        await poller.refreshNow()
        XCTAssertNil(poller.headlineUsedPercent, "a key without a limit has no percent")
        XCTAssertNil(poller.lastError)

        auth.signOut()
        XCTAssertNil(poller.snapshot)
    }

    func testPollIntervalFollowsMenuAndEnablement() async throws {
        auth.saveAPIKey("sk-or-v1-live")
        let snapshot = try await keySnapshot(usage: 1, limit: 10)
        let poller = OpenRouterUsagePoller(settings: settings, auth: auth) { _ in snapshot }
        poller.start()
        defer { poller.stop() }
        await poller.refreshNow()
        let idle = try XCTUnwrap(poller.pollingLoop.delayUntilDue())
        poller.menuIsOpen = true
        XCTAssertLessThan(try XCTUnwrap(poller.pollingLoop.delayUntilDue()), idle)
        settings.enabledProviderIDs = [.cursor]
        XCTAssertNil(poller.pollingLoop.delayUntilDue())
    }

    // MARK: - Live client

    func testLiveClientSendsTheBearerKeyAndReadsManagementEndpoints() async throws {
        HTTPStub.reset { request in
            switch request.url?.path {
            case "/api/v1/key": return .success(.json(#"{"data":{"usage":5,"limit":null,"is_management_key":true}}"#))
            case "/api/v1/credits": return .success(.json(#"{"data":{"total_credits":40,"total_usage":10}}"#))
            case "/api/v1/activity": return .success(.json(#"{"data":[]}"#))
            default: return .success(.json("{}", status: 404))
            }
        }
        let snapshot = try await OpenRouterUsageClient(apiKey: "sk-or-v1-x", session: HTTPStub.session()).fetchSnapshot()
        XCTAssertEqual(snapshot.usedPercent ?? -1, 25, accuracy: 0.001, "account credits are the denominator")
        let requests = HTTPStub.requests.map(\.request)
        XCTAssertEqual(Set(requests.compactMap { $0.url?.path }), ["/api/v1/key", "/api/v1/credits", "/api/v1/activity"])
        XCTAssertTrue(requests.allSatisfy { $0.value(forHTTPHeaderField: "Authorization") == "Bearer sk-or-v1-x" })
    }

    func testUnexpectedKeyJSONIsABadResponse() async {
        HTTPStub.reset(always: .json(#"{"data":"nope"}"#))
        do {
            _ = try await OpenRouterUsageClient(apiKey: "sk-or-v1-x", session: HTTPStub.session()).fetchSnapshot()
            XCTFail("expected a bad response")
        } catch {
            guard case let .badResponse(.openRouter, message) = error as? ProviderError else {
                return XCTFail("unexpected \(error)")
            }
            XCTAssertTrue(message.hasPrefix("Unexpected JSON"))
        }
    }

    func testBudgetSourceLabels() {
        XCTAssertEqual(OpenRouterBudgetSource.accountCredits.label, "Account credits")
        XCTAssertEqual(OpenRouterBudgetSource.keyLimit.label, "Key limit")
    }
}
