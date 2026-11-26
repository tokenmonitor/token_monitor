@testable import TokenMon
import XCTest

/// The sign-in sheet's state machine: manual and automatic capture, the one
/// retry, waiting out an OAuth popup, the confirmation and the dismissal.
@MainActor
final class ProviderSignInFlowTests: XCTestCase {
    /// Records every wait and lets a test act while the flow is suspended.
    private var waits: [UInt64] = []
    private var duringWait: ((UInt64) async -> Void)?
    private var completed = 0
    private var dismissed = 0

    override func setUp() async throws {
        waits = []
        duringWait = nil
        completed = 0
        dismissed = 0
    }

    private func makeFlow(
        _ auth: FakeSignInAuth,
        config: ProviderSignInConfig = .fixture(),
        browser: SignInBrowserController? = nil
    ) -> ProviderSignInFlow {
        let flow = ProviderSignInFlow(
            auth: auth,
            config: config,
            browser: browser ?? SignInBrowserController(),
            sleep: { [weak self] nanoseconds in
                self?.waits.append(nanoseconds)
                await self?.duringWait?(nanoseconds)
            },
            onComplete: { [weak self] in self?.completed += 1 }
        )
        flow.dismiss = { [weak self] in self?.dismissed += 1 }
        return flow
    }

    private func popup(_ browser: SignInBrowserController, open: Bool) {
        browser.apply(canGoBack: false, canGoForward: false, isLoading: false, currentURL: "", popupDepth: open ? 1 : 0)
    }

    // MARK: - Presentation

    func testStartsSigningInWithTheProviderSubtitle() {
        let flow = makeFlow(FakeSignInAuth())
        XCTAssertEqual(flow.phase, .signingIn)
        XCTAssertEqual(flow.statusText, "Sign in with your Example account.")
        XCTAssertEqual(flow.primaryTitle, "Finish Sign-In")
        XCTAssertFalse(flow.isBusy)
        XCTAssertFalse(flow.isSignedIn)
    }

    func testOpenPopupAsksToFinishThere() {
        let browser = SignInBrowserController()
        let flow = makeFlow(FakeSignInAuth(), browser: browser)
        popup(browser, open: true)
        XCTAssertEqual(flow.statusText, "Complete sign-in in the popup above.")
    }

    func testTimeoutHintNudgesTowardFinish() async {
        let flow = makeFlow(FakeSignInAuth())
        await flow.waitForTimeoutHint()
        XCTAssertEqual(waits, [ProviderSignInFlow.timeoutHintNanoseconds])
        XCTAssertTrue(flow.didHintTimeout)
        XCTAssertEqual(flow.statusText, "Signed in? Choose Finish Sign-In to complete.")
    }

    func testCancelledTimeoutWaitGivesNoHint() async {
        let flow = makeFlow(FakeSignInAuth())
        let task = Task { await flow.waitForTimeoutHint() }
        task.cancel()
        await task.value
        XCTAssertFalse(flow.didHintTimeout)
    }

    // MARK: - Manual finish

    func testFinishCapturesConfirmsThenDismisses() async {
        let auth = FakeSignInAuth(captureResults: [true], accountEmail: "a@b.com")
        var hookRan = false
        let flow = makeFlow(auth, config: .fixture(afterCapture: { hookRan = true }))
        var statusDuringConfirmation = ""
        duringWait = { _ in
            statusDuringConfirmation = flow.statusText
            XCTAssertEqual(flow.primaryTitle, "Signed In")
        }
        await flow.finish()

        XCTAssertEqual(flow.phase, .signedIn(email: "a@b.com"))
        XCTAssertEqual(statusDuringConfirmation, "Signed in as a@b.com")
        XCTAssertTrue(flow.isSignedIn)
        XCTAssertTrue(hookRan)
        XCTAssertEqual(completed, 1)
        XCTAssertEqual(waits, [ProviderSignInFlow.confirmationNanoseconds])
        XCTAssertEqual(dismissed, 1)
    }

    func testSignedInWithoutAnEmailSaysSo() async {
        let flow = makeFlow(FakeSignInAuth(captureResults: [true]))
        await flow.finish()
        XCTAssertEqual(flow.statusText, "Signed in")
    }

    func testFailedFinishOffersARetryWithTheAuthError() async {
        var hookRan = false
        let auth = FakeSignInAuth(captureResults: [false], lastAuthError: "No session cookie.")
        let flow = makeFlow(auth, config: .fixture(afterCapture: { hookRan = true }))
        await flow.finish()
        XCTAssertEqual(flow.phase, .failed("No session cookie."))
        XCTAssertEqual(flow.statusText, "No session cookie.")
        XCTAssertEqual(flow.primaryTitle, "Try Again")
        XCTAssertFalse(hookRan, "the after-capture hook runs only on success")
        XCTAssertEqual(completed, 0)
        XCTAssertEqual(dismissed, 0)
    }

    func testFailureWithoutAnAuthErrorUsesTheGenericMessage() async {
        let flow = makeFlow(FakeSignInAuth(captureResults: [false]))
        await flow.finish()
        XCTAssertEqual(flow.statusText, "Couldn't find a signed-in session yet. Finish signing in, then try again.")
    }

    func testFinishingShowsProgressWhileCapturing() async {
        let auth = FakeSignInAuth(captureResults: [true])
        let flow = makeFlow(auth)
        var seen: (busy: Bool, status: String, title: String)?
        auth.duringCapture = { seen = (flow.isBusy, flow.statusText, flow.primaryTitle) }
        await flow.finish()
        XCTAssertEqual(seen?.busy, true)
        XCTAssertEqual(seen?.status, "Finishing sign-in…")
        XCTAssertEqual(seen?.title, "Finishing…")
        XCTAssertTrue(flow.isSignedIn)
    }

    func testFinishDoesNothingOnceSignedInOrDismissed() async {
        let auth = FakeSignInAuth(captureResults: [true, true])
        let flow = makeFlow(auth)
        await flow.finish()
        await flow.finish()
        XCTAssertEqual(auth.captureCount, 1)

        let closed = makeFlow(FakeSignInAuth(captureResults: [true]))
        closed.markDismissed()
        await closed.finish()
        XCTAssertEqual(closed.phase, .signingIn)
    }

    /// A window closed during the confirmation is not dismissed again.
    func testWindowClosedDuringConfirmationIsNotDismissedAgain() async {
        let flow = makeFlow(FakeSignInAuth(captureResults: [true]))
        duringWait = { _ in flow.markDismissed() }
        await flow.finish()
        XCTAssertEqual(completed, 1)
        XCTAssertEqual(dismissed, 0)
    }

    // MARK: - Automatic capture

    func testReturnCapturesAfterTheProviderDelay() async {
        let auth = FakeSignInAuth(captureResults: [true])
        let flow = makeFlow(auth, config: .fixture(returnDelayNanoseconds: 500))
        await flow.captureAfterReturn()
        XCTAssertEqual(waits, [500, ProviderSignInFlow.confirmationNanoseconds])
        XCTAssertTrue(flow.isSignedIn)
        XCTAssertEqual(dismissed, 1)
    }

    func testLateCookiesAreRetriedOnce() async {
        let auth = FakeSignInAuth(captureResults: [false, true])
        let flow = makeFlow(auth)
        await flow.captureAfterReturn()
        XCTAssertEqual(auth.captureCount, 2)
        XCTAssertEqual(waits, [ProviderSignInFlow.retryNanoseconds, ProviderSignInFlow.confirmationNanoseconds])
        XCTAssertTrue(flow.isSignedIn)
    }

    func testTwoEmptyCapturesAskTheUser() async {
        let auth = FakeSignInAuth(captureResults: [false, false], lastAuthError: "Still no cookie.")
        let flow = makeFlow(auth)
        await flow.captureAfterReturn()
        XCTAssertEqual(flow.phase, .failed("Still no cookie."))
        XCTAssertEqual(completed, 0)
    }

    func testClosingDuringTheRetryWaitStopsTheRetry() async {
        let auth = FakeSignInAuth(captureResults: [false, true])
        let flow = makeFlow(auth)
        duringWait = { _ in flow.markDismissed() }
        await flow.captureAfterReturn()
        XCTAssertEqual(auth.captureCount, 1)
        XCTAssertFalse(flow.isSignedIn)
    }

    func testReturnWaitsForThePopupToClose() async {
        let browser = SignInBrowserController()
        let auth = FakeSignInAuth(captureResults: [true])
        let flow = makeFlow(auth, browser: browser)
        popup(browser, open: true)
        var polls = 0
        duringWait = { [weak self] nanoseconds in
            guard nanoseconds == ProviderSignInFlow.popupPollNanoseconds else { return }
            polls += 1
            if polls == 3 { self?.popup(browser, open: false) }
        }
        await flow.captureAfterReturn()
        XCTAssertEqual(polls, 3)
        XCTAssertTrue(flow.isSignedIn)
    }

    /// A popup that never closes is waited on a bounded number of times.
    func testPopupWaitIsBounded() async {
        let browser = SignInBrowserController()
        let flow = makeFlow(FakeSignInAuth(captureResults: [true]), browser: browser)
        popup(browser, open: true)
        await flow.captureAfterReturn()
        XCTAssertEqual(waits.filter { $0 == ProviderSignInFlow.popupPollNanoseconds }.count, ProviderSignInFlow.popupPollLimit)
        XCTAssertTrue(flow.isSignedIn)
    }

    func testClosingWhileThePopupIsOpenCapturesNothing() async {
        let browser = SignInBrowserController()
        let auth = FakeSignInAuth(captureResults: [true])
        let flow = makeFlow(auth, browser: browser)
        popup(browser, open: true)
        duringWait = { _ in flow.markDismissed() }
        await flow.captureAfterReturn()
        XCTAssertEqual(auth.captureCount, 0)
    }

    /// The user finished by hand while the return delay ran.
    func testManualFinishDuringTheReturnDelayWins() async {
        let auth = FakeSignInAuth(captureResults: [true])
        let flow = makeFlow(auth, config: .fixture(returnDelayNanoseconds: 500))
        var finishedByHand = false
        duringWait = { nanoseconds in
            guard nanoseconds == 500, !finishedByHand else { return }
            finishedByHand = true
            await flow.finish()
        }
        await flow.captureAfterReturn()
        XCTAssertTrue(finishedByHand)
        XCTAssertEqual(auth.captureCount, 1, "the automatic capture stands down")
        XCTAssertEqual(completed, 1)
    }

    func testReturnIgnoredOnceSignedIn() async {
        let auth = FakeSignInAuth(captureResults: [true, true])
        let flow = makeFlow(auth)
        await flow.finish()
        await flow.captureAfterReturn()
        XCTAssertEqual(auth.captureCount, 1)
    }
}
