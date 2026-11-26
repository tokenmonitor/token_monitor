@testable import TokenMon
import XCTest

/// The OpenRouter model breakdown ranks models by spend, then by tokens, and
/// identifies each by the slug OpenRouter reported.
final class OpenRouterModelRankingTests: XCTestCase {
    private func row(_ model: String, usage: Double, prompt: Int, completion: Int = 0) -> OpenRouterActivityRow {
        OpenRouterActivityRow(
            date: "2026-09-20",
            model: model,
            providerName: nil,
            usage: usage,
            requests: 1,
            promptTokens: prompt,
            completionTokens: completion,
            reasoningTokens: 0
        )
    }

    func testModelsRankBySpendThenTokens() {
        let models = OpenRouterModelUsage.models(from: [
            row("a/cheap", usage: 1, prompt: 10),
            row("b/pricey", usage: 3, prompt: 5),
            row("c/busy", usage: 1, prompt: 400, completion: 100)
        ])
        XCTAssertEqual(models.map(\.id), ["b/pricey", "c/busy", "a/cheap"])
        XCTAssertEqual(models[1].totalTokens, 500)
        XCTAssertEqual(models.map(\.percentOfWindow).reduce(0, +), 100, accuracy: 0.001)
    }

    func testSnapshotIsIdentifiedByItsFetchTime() throws {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let key = try JSONDecoder().decode(OpenRouterKeyData.self, from: Data(#"{"usage":1,"limit":10}"#.utf8))
        let snapshot = OpenRouterSnapshot.build(
            key: key,
            credits: nil,
            activity: nil,
            fetchedAt: now
        )
        XCTAssertEqual(snapshot.id, now)
    }
}
