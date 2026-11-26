import AppKit
import SwiftUI
@testable import TokenMon
import XCTest

/// Model ids map to the company behind them and its logo or letter.
@MainActor
final class ModelCompanyLogoTests: XCTestCase {
    private let companies: [ModelCompany] = [
        .kimi, .qwen, .glm, .minimax, .deepseek, .nvidia, .muse, .anthropic, .openai, .meta, .other
    ]

    func testModelIDsMapToTheirCompany() {
        let cases: [(String, ModelCompany)] = [
            ("meta/muse-spark", .meta),
            ("Muse_Spark-1", .meta),
            ("moonshot/kimi-k2", .kimi),
            ("Qwen3-Coder", .qwen),
            ("zai/GLM-4.6", .glm),
            ("xiaomi/mimo-v2", .minimax),
            ("MiniMax-M2", .minimax),
            ("deepseek-r1", .deepseek),
            ("nvidia/nemotron", .nvidia),
            ("muse-1", .muse),
            ("claude-opus-5", .anthropic),
            ("anthropic/foo", .anthropic),
            ("gpt-5", .openai),
            ("openai/o3", .openai),
            ("llama-4", .meta),
            ("meta/foo", .meta),
            ("mystery-model", .other)
        ]
        for (id, company) in cases {
            XCTAssertEqual(ModelCompany.forModelID(id), company, id)
        }
    }

    func testCompaniesWithALogoAsset() {
        let assets: [(ModelCompany, String?)] = [
            (.kimi, "KimiLogo"), (.qwen, "AlibabaLogo"), (.glm, "ZaiLogo"), (.minimax, "MinimaxLogo"),
            (.deepseek, "DeepSeekLogo"), (.nvidia, "NvidiaLogo"), (.muse, nil), (.anthropic, "AnthropicLogo"),
            (.openai, "OpenAILogo"), (.meta, "MetaLogo"), (.other, nil)
        ]
        for (company, asset) in assets {
            XCTAssertEqual(company.assetName, asset)
        }
    }

    /// Every logo asset the mapping names ships in the app bundle.
    func testEveryNamedLogoAssetExists() {
        for company in companies {
            guard let asset = company.assetName else { continue }
            XCTAssertNotNil(NSImage(named: asset), asset)
        }
    }

    func testLettersForTheLetteredCircle() {
        let letters: [(ModelCompany, String)] = [
            (.kimi, "K"), (.qwen, "Q"), (.glm, "Z"), (.minimax, "M"), (.deepseek, "D"), (.nvidia, "N"),
            (.muse, "M"), (.anthropic, "A"), (.openai, "O"), (.meta, "M"), (.other, "•")
        ]
        for (company, letter) in letters {
            XCTAssertEqual(company.letter, letter)
        }
    }

    /// Only Z.ai's white circle takes a black letter; every company has its
    /// own background.
    func testLogoColours() {
        for company in companies {
            XCTAssertEqual(company.logoForeground, company == .glm ? .black : .white)
        }
        XCTAssertEqual(ModelCompany.glm.logoBackground, .white)
        let backgrounds = companies.map(\.logoBackground)
        XCTAssertEqual(Set(backgrounds.map(\.description)).count, companies.count)
    }

    /// A company with an asset draws the image; one without draws its letter.
    func testLogoDrawsTheAssetOrTheLetter() {
        for (id, asset) in [("deepseek-r1", "DeepSeekLogo"), ("gpt-5", "OpenAILogo"), ("kimi-k2", "KimiLogo")] {
            let view = ModelCompanyLogo(modelID: id)
            XCTAssertEqual(ViewHarness.renderBothAppearances(view, width: 28).height, 28, accuracy: 0.5)
            XCTAssertEqual(ViewTree(view).texts, [asset], id)
        }
        let muse = ModelCompanyLogo(modelID: "muse-1")
        XCTAssertEqual(ViewHarness.renderBothAppearances(muse, width: 28).height, 28, accuracy: 0.5)
        XCTAssertEqual(ViewTree(muse).texts, ["M"])
        XCTAssertEqual(ViewTree(ModelCompanyLogo(modelID: "unknown")).texts, ["•"])
    }
}
