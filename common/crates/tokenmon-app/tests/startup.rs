//! Startup failure handling (#69).

use tokenmon_app::startup::{PendingShow, StartupFailure, retry};

/// The tray retry stops at the first success and sleeps only between tries.
#[test]
fn test_retry_stops_at_first_success() {
    let mut sleeps = Vec::new();
    let mut calls = 0;
    let r: Result<&str, &str> = retry(
        5,
        |n| sleeps.push(n),
        || {
            calls += 1;
            if calls == 3 { Ok("built") } else { Err("busy") }
        },
    );
    assert_eq!(r, Ok("built"));
    assert_eq!(sleeps, [1, 2]);
}

/// After the last attempt the error is returned, with no trailing sleep.
#[test]
fn test_retry_gives_up_with_the_last_error() {
    let mut sleeps = 0;
    let mut calls = 0;
    let r: Result<(), u32> = retry(
        3,
        |_| sleeps += 1,
        || {
            calls += 1;
            Err(calls)
        },
    );
    assert_eq!(r, Err(3));
    assert_eq!(sleeps, 2);
}

/// Zero attempts still tries once.
#[test]
fn test_retry_with_zero_attempts_runs_once() {
    let mut calls = 0;
    let r: Result<(), ()> = retry(
        0,
        |_| {},
        || {
            calls += 1;
            Err(())
        },
    );
    assert!(r.is_err());
    assert_eq!(calls, 1);
}

/// Each failure explains itself and names where the log is.
#[test]
fn test_failure_messages_name_the_cause_and_the_log() {
    let tray = StartupFailure::Tray("explorer".into()).message();
    assert!(tray.contains("tray icon") && tray.contains("explorer") && tray.contains("logs"));
    let pipe = StartupFailure::Pipe("denied".into()).message();
    assert!(pipe.contains("single-instance") && pipe.contains("denied"));
}

/// The log folder named in a failure message follows the OS family: the
/// Windows text is unchanged, Unix names the XDG data directory.
#[test]
fn test_failure_messages_name_the_log_folder_of_the_os() {
    use tokenmon_core::storage::HostOs;
    let failure = StartupFailure::Assembly("boom".into());
    let windows = failure.message_for(HostOs::Windows);
    assert!(
        windows.ends_with(r"The log is in %LOCALAPPDATA%\TokenMon\logs."),
        "{windows}"
    );
    let unix = failure.message_for(HostOs::Unix);
    assert!(unix.contains("$XDG_DATA_HOME/TokenMon/logs"), "{unix}");
    assert_eq!(failure.message(), failure.message_for(HostOs::current()));
}

/// A show request before the UI is ready is kept and handed over exactly once.
#[test]
fn test_show_request_before_ready_is_delivered_once() {
    let p = PendingShow::new();
    assert!(!p.request(), "UI not ready: caller must not post");
    assert!(p.mark_ready());
    assert!(!p.take());
}

/// After the UI is ready a request is posted by the caller and consumed once
/// even if the posted task and the ready check race.
#[test]
fn test_show_request_after_ready_is_posted_and_taken_once() {
    let p = PendingShow::new();
    assert!(!p.mark_ready());
    assert!(p.request());
    assert!(p.take());
    assert!(!p.take());
}
