import SwiftUI
@testable import TokenMon
import XCTest

/// The OpenRouter panel shows the credit budget, stats and model spend for
/// each key state, and its key field saves, replaces and signs out the key.
@MainActor
final class OpenRouterPanelViewTests: XCTestCase {
    private var dir: URL!
    private var suiteName: String!
    private var defaults: UserDefaults!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        suiteName = "OpenRouterPanel-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
        defaults.removePersistentDomain(forName: suiteName)
    }

    private func model(_ id: String, cost: Double, estimated: Bool = false, revealed: Bool = false) -> OpenRouterModelUsage {
        OpenRouterModelUsage(
            modelID: id,
            activitySlug: revealed ? "stealth/\(id)" : id,
            requests: 1,
            promptTokens: 10,
            completionTokens: 5,
            costUSD: cost,
            isCostEstimated: estimated,
            percentOfWindow: 50
        )
    }

    /// Account credits of $100 with $42.40 spent, as the Rust panel fixture.
    private func creditsSnapshot() -> OpenRouterSnapshot {
        var snapshot = OpenRouterSnapshot(
            fetchedAt: Date(),
            keyLabel: "main",
            isManagementKey: true,
            isFreeTier: false,
            accountCreditsUSD: 100,
            accountUsedUSD: 42.4,
            keyUsageUSD: 10,
            keyUsageDailyUSD: 1.25,
            keyUsageWeeklyUSD: 3,
            keyUsageMonthlyUSD: 8,
            keyLimitUSD: nil,
            keyLimitRemainingUSD: nil,
            budgetSource: .accountCredits,
            budgetUSD: 100,
            usedUSD: 42.4,
            remainingUSD: 57.6
        )
        snapshot.models = [model("z-ai/glm-5.3-flash", cost: 1, estimated: true, revealed: true)]
        return snapshot
    }

    private func spendOnlySnapshot() -> OpenRouterSnapshot {
        var snapshot = creditsSnapshot()
        snapshot.budgetSource = nil
        snapshot.budgetUSD = nil
        snapshot.accountUsedUSD = nil
        snapshot.remainingUSD = nil
        snapshot.models = []
        return snapshot
    }

    private func makeAuth(signedIn: Bool = true) -> OpenRouterAuthSession {
        let auth = OpenRouterAuthSession(directory: dir, store: InMemoryCredentialStore())
        if signedIn {
            XCTAssertTrue(auth.saveAPIKey("sk-or-v1-test"))
        }
        return auth
    }

    private func makePoller(
        _ auth: OpenRouterAuthSession,
        fetch: @escaping (String) async throws -> OpenRouterSnapshot
    ) -> OpenRouterUsagePoller {
        OpenRouterUsagePoller(settings: AppSettings(defaults: defaults), auth: auth, fetchSnapshot: fetch)
    }

    private func loadedPoller(_ auth: OpenRouterAuthSession, _ snapshot: OpenRouterSnapshot) async -> OpenRouterUsagePoller {
        let poller = makePoller(auth) { _ in snapshot }
        await poller.refreshNow()
        XCTAssertNotNil(poller.snapshot)
        return poller
    }

    func testSignedOutPanelAsksForAKey() {
        let auth = makeAuth(signedIn: false)
        let poller = makePoller(auth) { _ in self.creditsSnapshot() }
        let view = OpenRouterPanelView(poller: poller, auth: auth)
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Paste an OpenRouter API key to track spending against your purchased credits."))
        XCTAssertTrue(tree.shows("Add Key"))
        XCTAssertFalse(tree.shows("Save Key"))
        XCTAssertFalse(tree.shows("Replace API Key…"))
    }

    func testAddKeyWithInvalidDraftShowsTheAuthError() {
        let auth = makeAuth(signedIn: false)
        let poller = makePoller(auth) { _ in self.creditsSnapshot() }
        let view = OpenRouterPanelView(poller: poller, auth: auth, apiKeyDraft: "not-a-key")
        ViewTree(view).press("Add Key")
        XCTAssertFalse(auth.isSignedIn)
        let error = try? XCTUnwrap(auth.lastAuthError)
        XCTAssertNotNil(error)
        let after = OpenRouterPanelView(poller: poller, auth: auth)
        ViewHarness.render(after)
        XCTAssertTrue(ViewTree(after).shows(error ?? "-"))
    }

    func testAddKeySavesTheKeyAndRefreshes() async {
        let auth = makeAuth(signedIn: false)
        let fetched = expectation(description: "refresh after save")
        let poller = makePoller(auth) { key in
            XCTAssertEqual(key, "sk-or-v1-new")
            fetched.fulfill()
            return self.creditsSnapshot()
        }
        let view = OpenRouterPanelView(poller: poller, auth: auth, apiKeyDraft: "  sk-or-v1-new \n")
        ViewTree(view).press("Add Key")
        XCTAssertTrue(auth.isSignedIn)
        XCTAssertEqual(auth.apiKey(), "sk-or-v1-new")
        await fulfillment(of: [fetched], timeout: 5)
    }

    func testCreditBudgetStatsAndModels() async {
        let auth = makeAuth()
        let poller = await loadedPoller(auth, creditsSnapshot())
        let view = OpenRouterPanelView(poller: poller, auth: auth)
        ViewHarness.renderBothAppearances(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Account credits"))
        XCTAssertTrue(tree.shows("Credit Budget"))
        XCTAssertTrue(tree.shows("42% Used"))
        XCTAssertTrue(tree.shows("$42.40 of $100.00 · $57.60 left"))
        for text in ["Credits purchased", "$100.00", "Total spent", "Remaining", "$57.60", "Spent today", "$1.25"] {
            XCTAssertTrue(tree.shows(text), text)
        }
        XCTAssertTrue(tree.shows("Top Models by Spend"))
        XCTAssertTrue(tree.shows("z-ai/glm-5.3-flash"))
        XCTAssertFalse(tree.shows("View all"))
        XCTAssertTrue(tree.shows("Replace API Key…"))
        XCTAssertFalse(tree.shows("Add Key"))
    }

    func testKeyLimitTitlesTheBudgetStat() async {
        var snapshot = creditsSnapshot()
        snapshot.budgetSource = .keyLimit
        snapshot.limitReset = "monthly"
        snapshot.resetsAt = Date().addingTimeInterval(86400)
        let auth = makeAuth()
        let poller = await loadedPoller(auth, snapshot)
        let view = OpenRouterPanelView(poller: poller, auth: auth)
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("Key limit"))
        XCTAssertTrue(tree.shows("Key spending limit"))
    }

    func testNoCreditLimitShowsSpendOnly() async {
        let auth = makeAuth()
        let poller = await loadedPoller(auth, spendOnlySnapshot())
        let view = OpenRouterPanelView(poller: poller, auth: auth)
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("No credit limit"))
        XCTAssertTrue(tree.shows("This key has no credit limit, so usage shows as spend only."))
        XCTAssertFalse(tree.shows("% Used"))
        XCTAssertTrue(tree.shows("Credit limit"))
        XCTAssertTrue(tree.shows("None"))
        // Total spent falls back to the key's own usage; remaining is zero.
        XCTAssertTrue(tree.shows("$10.00"))
        XCTAssertTrue(tree.shows("$0.00"))
        XCTAssertFalse(tree.shows("Top Models by Spend"))
    }

    func testFailedRefreshKeepsDataWithAStaleCaption() async {
        let auth = makeAuth()
        var fail = false
        let poller = makePoller(auth) { _ in
            if fail { throw URLError(.timedOut) }
            return self.creditsSnapshot()
        }
        await poller.refreshNow()
        fail = true
        await poller.refreshNow()
        let error = try? XCTUnwrap(poller.lastError)
        XCTAssertNotNil(poller.snapshot)
        let view = OpenRouterPanelView(poller: poller, auth: auth)
        ViewHarness.render(view)
        XCTAssertTrue(ViewTree(view).shows("42% Used"))
        XCTAssertNotNil(ViewTree(view).first(StaleDataCaption.self))
        XCTAssertEqual(ViewTree(view).first(StaleDataCaption.self)?.message, error)
    }

    func testReplaceKeyRowSwapsTheKeyInPlace() async {
        let auth = makeAuth()
        let poller = await loadedPoller(auth, creditsSnapshot())
        let replacing = OpenRouterPanelView(poller: poller, auth: auth, apiKeyDraft: "sk-or-v1-rotated", isReplacingKey: true)
        ViewHarness.render(replacing)
        let tree = ViewTree(replacing)
        XCTAssertFalse(tree.shows("Replace API Key…"))
        tree.press("Save Key")
        XCTAssertEqual(auth.apiKey(), "sk-or-v1-rotated")

        // The link opens the field (its state writes are outside the render
        // graph here, so the press only has to run without side effects).
        let idle = OpenRouterPanelView(poller: poller, auth: auth)
        ViewTree(idle).press("Replace API Key…")
        XCTAssertEqual(auth.apiKey(), "sk-or-v1-rotated")
    }

    func testRejectedKeyShowsTheKeyFieldBesideTheData() async {
        let auth = makeAuth()
        let poller = await loadedPoller(auth, creditsSnapshot())
        auth.markSessionInvalid(reason: "Key rejected")
        let view = OpenRouterPanelView(poller: poller, auth: auth)
        ViewHarness.render(view)
        let tree = ViewTree(view)
        XCTAssertTrue(tree.shows("42% Used"))
        XCTAssertTrue(tree.shows("Add Key"))
        XCTAssertFalse(tree.shows("Replace API Key…"))
    }

    func testSignOutClearsTheKeyAndData() async {
        let auth = makeAuth()
        let poller = await loadedPoller(auth, creditsSnapshot())
        let view = OpenRouterPanelView(poller: poller, auth: auth)
        ViewTree(view).first(ProviderSignOutButton.self)?.action()
        XCTAssertNil(auth.apiKey())
        XCTAssertNil(poller.snapshot)
    }

    func testSignedInWithoutDataShowsStatusAndKeyField() async {
        let auth = makeAuth()
        let poller = makePoller(auth) { _ in throw URLError(.notConnectedToInternet) }
        let waiting = OpenRouterPanelView(poller: poller, auth: auth)
        ViewHarness.render(waiting)
        XCTAssertTrue(ViewTree(waiting).shows("No usage data yet."))
        XCTAssertTrue(ViewTree(waiting).shows("Add Key"))

        await poller.refreshNow()
        let error = try? XCTUnwrap(poller.lastError)
        let failed = OpenRouterPanelView(poller: poller, auth: auth)
        ViewHarness.render(failed)
        XCTAssertTrue(ViewTree(failed).shows(error ?? "-"))
    }

    func testModelsListPreviewsThreeAndExpands() {
        let models = (1 ... 5).map { model("vendor/model-\($0)", cost: Double(6 - $0)) }
        let preview = OpenRouterModelsSection(models: models)
        ViewHarness.render(preview)
        let tree = ViewTree(preview)
        XCTAssertTrue(tree.shows("vendor/model-3"))
        XCTAssertFalse(tree.shows("vendor/model-4"))
        tree.press("View all")

        let all = OpenRouterModelsSection(models: models, showAll: true)
        let expanded = ViewHarness.render(all)
        let allTree = ViewTree(all)
        XCTAssertTrue(allTree.shows("vendor/model-5"))
        XCTAssertTrue(allTree.shows("Show less"))
        XCTAssertGreaterThan(expanded.height, ViewHarness.render(preview).height)
    }

    func testModelRowTagsRevealedModelsAndEstimatedCosts() {
        let revealed = OpenRouterModelRow(model: model("z-ai/glm-5.3-flash", cost: 1, estimated: true, revealed: true))
        ViewHarness.renderBothAppearances(revealed)
        let tree = ViewTree(revealed)
        XCTAssertTrue(tree.shows("z-ai/glm-5.3-flash"))
        XCTAssertTrue(tree.shows("revealed"))
        XCTAssertTrue(tree.shows("~$1.00"))

        let billed = ViewTree(OpenRouterModelRow(model: model("openai/gpt-5", cost: 2.5)))
        XCTAssertTrue(billed.shows("$2.50"))
        XCTAssertFalse(billed.shows("~"))
        XCTAssertFalse(billed.shows("revealed"))
    }
}
