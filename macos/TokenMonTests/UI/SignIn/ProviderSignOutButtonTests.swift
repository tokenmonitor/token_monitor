import SwiftUI
@testable import TokenMon
import XCTest

/// Sign-out needs a confirmation; sign-in names the provider unless titled.
@MainActor
final class ProviderSignOutButtonTests: XCTestCase {
    func testStartsAsAnIconThatAsksForConfirmation() {
        var signedOut = 0
        let button = ProviderSignOutButton(provider: .claude) { signedOut += 1 }
        ViewHarness.renderBothAppearances(button)
        let tree = ViewTree(button)
        XCTAssertTrue(tree.shows("Sign out"))
        XCTAssertFalse(tree.shows("Confirm sign out"))
        tree.press("Sign out")
        XCTAssertEqual(signedOut, 0, "the first press only asks")
    }

    func testConfirmingSignsOut() {
        var signedOut = 0
        let button = ProviderSignOutButton(provider: .claude, isConfirming: true) { signedOut += 1 }
        ViewHarness.renderBothAppearances(button)
        let tree = ViewTree(button)
        XCTAssertTrue(tree.shows("Sign out of %@?"))
        XCTAssertTrue(tree.texts.contains("Claude"))
        tree.press("Confirm sign out")
        XCTAssertEqual(signedOut, 1)
    }

    func testCancellingKeepsTheSession() {
        var signedOut = 0
        let tree = ViewTree(ProviderSignOutButton(provider: .grok, isConfirming: true) { signedOut += 1 })
        tree.press("Cancel")
        XCTAssertEqual(signedOut, 0)
    }

    func testSignInButtonNamesTheProviderUnlessTitled() {
        var opened = 0
        let standard = ProviderSignInButton(provider: .cursor) { opened += 1 }
        ViewHarness.render(standard)
        ViewTree(standard).press("Sign In to Cursor…")
        XCTAssertEqual(opened, 1)

        let titled = ProviderSignInButton(provider: .cursor, title: "Sign In Again…") { opened += 1 }
        ViewTree(titled).press("Sign In Again…")
        XCTAssertEqual(opened, 2)
    }
}
