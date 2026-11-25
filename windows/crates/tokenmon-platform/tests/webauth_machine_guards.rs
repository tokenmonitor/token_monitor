//! Direct reducer tests for `SignInMachine`: guard conditions, popup polling,
//! stale timers and the dismissed latch.

#![allow(clippy::assert_is_empty)]

mod webauth_common;

use tokenmon_platform::webauth::machine::{
    Action, Input, POPUP_POLL_INTERVAL, POPUP_POLL_MAX, Phase, SignInMachine, TimerId, ViewKind,
};
use tokenmon_platform::webauth::nav::NavSnapshot;
use webauth_common::*;

fn machine() -> SignInMachine {
    SignInMachine::new(&claude_config())
}

fn go(m: &mut SignInMachine, view: ViewKind, url: &str) -> Vec<Action> {
    m.handle(Input::NavigationFinished {
        view,
        url: url.into(),
    })
}

/// Start page, login page, then back to a return page: the main view returns.
fn return_to_app(m: &mut SignInMachine) -> Vec<Action> {
    go(m, ViewKind::Main, "https://claude.ai/new");
    go(m, ViewKind::Main, "https://claude.ai/login");
    go(m, ViewKind::Main, "https://claude.ai/new")
}

fn has_timer(actions: &[Action], id: TimerId) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::StartTimer { id: t, .. } if *t == id))
}

fn good_cookies() -> Vec<tokenmon_platform::webauth::WebCookie> {
    vec![
        cookie("sessionKey", "sk", ".claude.ai"),
        cookie("lastActiveOrg", "org", ".claude.ai"),
    ]
}

/// Adopting without a resolved e-mail shows the plain "Signed in" status and
/// the check mark.
#[test]
fn test_signed_in_without_email_has_plain_status() {
    let mut m = machine();
    m.handle(Input::FinishClicked);
    m.handle(Input::CookiesRead(good_cookies()));
    m.handle(Input::AdoptFinished(Ok(None)));
    assert_eq!(m.status_text(), "Signed in");
    assert!(m.view_model().show_check);
    assert!(!m.view_model().primary_enabled);
}

/// The nav accessor mirrors the last snapshot the host reported.
#[test]
fn test_nav_accessor_reflects_snapshot_and_popups() {
    let mut m = machine();
    let snap = NavSnapshot {
        can_go_back: true,
        can_go_forward: true,
        is_loading: true,
        current_url: Some("https://claude.ai/login".into()),
    };
    m.handle(Input::NavSnapshot(snap.clone()));
    m.handle(Input::PopupOpened);
    assert_eq!(m.nav().snapshot, snap);
    assert_eq!(m.popup_depth(), 1);
    let vm = m.view_model();
    assert!(vm.back_enabled && vm.forward_enabled && vm.is_loading && vm.show_close_popup);
    assert_eq!(vm.address, "Sign-in popup");
    assert_eq!(vm.full_url, "https://claude.ai/login");
}

/// After the window closed every input is ignored, even a second close.
#[test]
fn test_dismissed_machine_ignores_everything_after_close() {
    let mut m = machine();
    let first = m.handle(Input::WindowClosed);
    assert!(m.is_dismissed());
    assert!(first.contains(&Action::CancelTimer(TimerId::ConfirmDismiss)));
    assert!(m.handle(Input::FinishClicked).is_empty());
    assert!(m.handle(Input::Opened).is_empty());
    assert!(m.handle(Input::TimerFired(TimerId::StatusHint)).is_empty());
    assert!(
        m.handle(Input::WindowClosed)
            .contains(&Action::CancelTimer(TimerId::StatusHint))
    );
}

/// Back goes back, then closes popups, and does nothing with neither.
#[test]
fn test_back_click_prefers_history_then_popups_then_nothing() {
    let mut m = machine();
    assert!(!m.handle(Input::BackClicked).contains(&Action::GoBack));
    m.handle(Input::PopupOpened);
    assert!(m.handle(Input::BackClicked).contains(&Action::ClosePopup));
    m.handle(Input::NavSnapshot(NavSnapshot {
        can_go_back: true,
        ..NavSnapshot::default()
    }));
    assert!(m.handle(Input::BackClicked).contains(&Action::GoBack));
}

/// The "Close popup" button acts only while a popup exists.
#[test]
fn test_close_popup_click_requires_a_popup() {
    let mut m = machine();
    assert!(
        !m.handle(Input::ClosePopupClicked)
            .contains(&Action::ClosePopup)
    );
    m.handle(Input::PopupOpened);
    assert!(
        m.handle(Input::ClosePopupClicked)
            .contains(&Action::ClosePopup)
    );
}

/// Cancel always asks the host to close the window.
#[test]
fn test_cancel_click_closes_the_window() {
    let mut m = machine();
    assert!(
        m.handle(Input::CancelClicked)
            .contains(&Action::CloseWindow)
    );
}

/// A popup closing after the main view already returned re-fires nothing
/// (the latch holds), and closing with no popups never underflows.
#[test]
fn test_popup_closed_does_not_refire_the_return_latch() {
    let mut m = machine();
    let first = return_to_app(&mut m);
    assert!(first.iter().any(|a| matches!(a, Action::OnReturned(_))));
    m.handle(Input::PopupOpened);
    m.handle(Input::PopupOpened);
    let still_open = m.handle(Input::PopupClosed);
    assert_eq!(m.popup_depth(), 1);
    assert!(
        !still_open
            .iter()
            .any(|a| matches!(a, Action::OnReturned(_)))
    );
    let closed = m.handle(Input::PopupClosed);
    assert!(!closed.iter().any(|a| matches!(a, Action::OnReturned(_))));
    m.handle(Input::PopupClosed);
    assert_eq!(m.popup_depth(), 0);
}

/// The return is deferred while a popup is open and fires when it closes.
#[test]
fn test_return_deferred_until_popup_closes() {
    let mut m = machine();
    go(&mut m, ViewKind::Main, "https://claude.ai/new");
    go(&mut m, ViewKind::Main, "https://claude.ai/login");
    m.handle(Input::PopupOpened);
    let deferred = go(&mut m, ViewKind::Main, "https://claude.ai/new");
    assert!(!deferred.iter().any(|a| matches!(a, Action::OnReturned(_))));
    let fired = m.handle(Input::PopupClosed);
    assert!(fired.iter().any(|a| matches!(a, Action::OnReturned(_))));
    assert!(has_timer(&fired, TimerId::ReturnDelay));
}

/// A popup closing while the main view sits on a non-return page fires nothing.
#[test]
fn test_popup_closed_off_the_return_page_fires_nothing() {
    let mut m = machine();
    return_to_app(&mut m);
    go(&mut m, ViewKind::Main, "https://claude.ai/login");
    m.handle(Input::PopupOpened);
    let out = m.handle(Input::PopupClosed);
    assert!(!out.iter().any(|a| matches!(a, Action::OnReturned(_))));
    let mut fresh = machine();
    fresh.handle(Input::PopupOpened);
    assert!(
        !fresh
            .handle(Input::PopupClosed)
            .iter()
            .any(|a| matches!(a, Action::OnReturned(_)))
    );
}

/// A popup-view navigation to the return page marks the return but does not
/// itself fire it, and an unparsable URL is ignored.
#[test]
fn test_popup_navigation_and_bad_urls_do_not_fire_return() {
    let mut m = machine();
    go(&mut m, ViewKind::Main, "https://claude.ai/new");
    go(&mut m, ViewKind::Popup, "https://claude.ai/login");
    let out = go(&mut m, ViewKind::Popup, "https://claude.ai/new");
    assert!(!out.iter().any(|a| matches!(a, Action::OnReturned(_))));
    assert!(go(&mut m, ViewKind::Main, "not a url").is_empty());
}

/// An in-flight manual capture suppresses the post-return delay.
#[test]
fn test_return_during_manual_capture_skips_the_return_delay() {
    let mut m = machine();
    m.handle(Input::FinishClicked);
    let out = return_to_app(&mut m);
    assert!(out.iter().any(|a| matches!(a, Action::OnReturned(_))));
    assert!(!has_timer(&out, TimerId::ReturnDelay));
}

/// The popup wait polls every 200 ms, then gives up after the budget and
/// captures anyway.
#[test]
fn test_popup_wait_polls_until_budget_then_captures() {
    let mut m = machine();
    return_to_app(&mut m);
    m.handle(Input::PopupOpened);
    let started = m.handle(Input::TimerFired(TimerId::ReturnDelay));
    assert!(has_timer(&started, TimerId::PopupPoll));
    let mut last = vec![];
    for _ in 0..POPUP_POLL_MAX - 1 {
        last = m.handle(Input::TimerFired(TimerId::PopupPoll));
        assert!(last.contains(&Action::StartTimer {
            id: TimerId::PopupPoll,
            delay: POPUP_POLL_INTERVAL
        }));
    }
    let out = m.handle(Input::TimerFired(TimerId::PopupPoll));
    assert!(out.contains(&Action::ReadCookies), "{last:?}");
    assert_eq!(m.phase(), &Phase::Finishing);
}

/// The popup wait captures as soon as the popup is gone.
#[test]
fn test_popup_wait_captures_when_popup_closes() {
    let mut m = machine();
    return_to_app(&mut m);
    m.handle(Input::PopupOpened);
    m.handle(Input::TimerFired(TimerId::ReturnDelay));
    m.handle(Input::PopupClosed);
    let out = m.handle(Input::TimerFired(TimerId::PopupPoll));
    assert!(out.contains(&Action::ReadCookies));
}

/// Timers that arrive in the wrong flow state are ignored.
#[test]
fn test_stale_timers_are_ignored() {
    let mut m = machine();
    for id in [
        TimerId::PopupPoll,
        TimerId::ReturnDelay,
        TimerId::CaptureAttempt,
        TimerId::AutoRetry,
        TimerId::ConfirmDismiss,
    ] {
        let out = m.handle(Input::TimerFired(id));
        assert!(
            out.iter().all(|a| matches!(a, Action::UpdateView(_))),
            "{id:?}: {out:?}"
        );
    }
    assert_eq!(m.phase(), &Phase::SigningIn);
    assert!(m.handle(Input::CookiesRead(good_cookies())).is_empty());
    assert!(m.handle(Input::AdoptFinished(Ok(None))).is_empty());
    // A return delay that fires after a manual finish is stale too.
    return_to_app(&mut m);
    m.handle(Input::FinishClicked);
    let out = m.handle(Input::TimerFired(TimerId::ReturnDelay));
    assert!(!out.contains(&Action::ReadCookies));
}

/// The status hint appears after its timer, and a popup message outranks it.
#[test]
fn test_status_text_priority() {
    let mut m = machine();
    let subtitle = m.status_text();
    m.handle(Input::TimerFired(TimerId::StatusHint));
    assert_eq!(
        m.status_text(),
        "Signed in? Choose Finish Sign-In to complete."
    );
    m.handle(Input::PopupOpened);
    assert_eq!(m.status_text(), "Complete sign-in in the popup above.");
    assert_ne!(subtitle, m.status_text());
}

/// A manual capture that fails shows the policy message, enables retry and
/// re-arms the return latch so the next return fires again.
#[test]
fn test_manual_failure_rearms_the_return() {
    let mut m = machine();
    m.handle(Input::FinishClicked);
    for _ in 0..4 {
        m.handle(Input::CookiesRead(vec![]));
        m.handle(Input::TimerFired(TimerId::CaptureAttempt));
    }
    m.handle(Input::CookiesRead(vec![]));
    assert!(matches!(m.phase(), Phase::Failed { .. }));
    assert_eq!(m.view_model().primary_title, "Try Again");
    let out = return_to_app(&mut m);
    assert!(out.iter().any(|a| matches!(a, Action::OnReturned(_))));
}

/// An adopt error with an empty message falls back to the policy message;
/// a non-empty one is shown as-is.
#[test]
fn test_adopt_errors_surface_message_or_policy_default() {
    let mut m = machine();
    m.handle(Input::FinishClicked);
    m.handle(Input::CookiesRead(good_cookies()));
    m.handle(Input::AdoptFinished(Err(String::new())));
    let Phase::Failed { message } = m.phase().clone() else {
        panic!("expected failure, got {:?}", m.phase());
    };
    assert!(!message.is_empty());
    m.handle(Input::FinishClicked);
    m.handle(Input::CookiesRead(good_cookies()));
    m.handle(Input::AdoptFinished(Err("disk full".into())));
    assert_eq!(
        m.phase(),
        &Phase::Failed {
            message: "disk full".into()
        }
    );
}

/// An automatic capture retries exactly once after the retry delay.
#[test]
fn test_automatic_capture_retries_once() {
    let mut m = machine();
    return_to_app(&mut m);
    m.handle(Input::TimerFired(TimerId::ReturnDelay));
    for _ in 0..3 {
        m.handle(Input::CookiesRead(vec![]));
        let again = m.handle(Input::TimerFired(TimerId::CaptureAttempt));
        assert!(again.contains(&Action::ReadCookies));
    }
    let waiting = m.handle(Input::CookiesRead(vec![]));
    assert!(has_timer(&waiting, TimerId::AutoRetry));
    assert_eq!(m.phase(), &Phase::Finishing);
    assert!(
        m.handle(Input::TimerFired(TimerId::AutoRetry))
            .contains(&Action::ReadCookies)
    );
    for _ in 0..3 {
        m.handle(Input::CookiesRead(vec![]));
        m.handle(Input::TimerFired(TimerId::CaptureAttempt));
    }
    m.handle(Input::CookiesRead(vec![]));
    assert!(matches!(m.phase(), Phase::Failed { .. }));
}
