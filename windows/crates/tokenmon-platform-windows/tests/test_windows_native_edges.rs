//! Edge behavior of the real Windows calls: waiting on live and finished
//! processes, pipe clients that misbehave in ways the happy-path tests do not
//! reach, and the DPAPI size limit. Hermetic (per-process pipe names, child
//! processes that exit on their own).
#![cfg(windows)]
#![allow(clippy::unwrap_used)]

use std::fs::OpenOptions;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tokenmon_platform::shell::ShellError;
use tokenmon_platform::shell::ipc::{IpcClient, IpcMessage};
use tokenmon_platform_windows::install::wait_for_pid_exit;
use tokenmon_platform_windows::secrets::blob_len;
use tokenmon_platform_windows::shell::windows::{PipeClient, PipeServer, current_user_pipe_name};

fn pipe_name(tag: &str) -> String {
    format!(
        "{}.edges{}.{tag}",
        current_user_pipe_name().expect("the test user has a SID and a session"),
        std::process::id()
    )
}

/// Waiting on a process that is still running times out and reports `false`:
/// the caller must not assume the old build is gone.
#[test]
fn test_wait_for_a_live_process_times_out() {
    let started = Instant::now();
    assert!(!wait_for_pid_exit(
        std::process::id(),
        Duration::from_millis(100)
    ));
    assert!(started.elapsed() >= Duration::from_millis(90));
}

/// Waiting on a process that has exited returns `true` at once, even while
/// its handle is still open (the exit is what is waited for).
#[test]
fn test_wait_for_a_finished_process_returns_true() {
    let mut child = std::process::Command::new("cmd")
        .args(["/C", "exit", "0"])
        .spawn()
        .expect("cmd starts");
    child.wait().expect("child exits");
    assert!(wait_for_pid_exit(child.id(), Duration::from_secs(5)));
}

/// A pipe name Windows rejects is a typed IPC error straight away, not five
/// seconds of retrying as if the server were merely starting.
#[test]
fn test_client_with_an_invalid_pipe_name_fails_without_retrying() {
    let started = Instant::now();
    let err = PipeClient::new("C:\\tokenmon<invalid>")
        .send(&IpcMessage::Show)
        .unwrap_err();
    assert!(matches!(err, ShellError::Ipc(_)), "{err:?}");
    assert!(started.elapsed() < Duration::from_secs(3));
}

/// A client that connects, stays silent while the server is already reading
/// and then closes ends that exchange without a message; the next client is
/// still served.
#[test]
fn test_client_closing_while_the_server_reads_is_ignored() {
    let name = pipe_name("closing");
    let (tx, rx) = mpsc::channel();
    let server = PipeServer::start(&name, move |m| {
        let _ = tx.send(m);
    })
    .expect("server starts");

    let deadline = Instant::now() + Duration::from_secs(10);
    let silent = loop {
        match OpenOptions::new().write(true).open(&name) {
            Ok(f) => break f,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => panic!("pipe never became free: {e}"),
        }
    };
    // Let the server reach its pending read before the client goes away.
    std::thread::sleep(Duration::from_millis(300));
    drop(silent);

    PipeClient::new(name).send(&IpcMessage::Show).expect("send");
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(10)),
        Ok(IpcMessage::Show)
    );
    assert!(rx.try_recv().is_err(), "the silent client produced nothing");
    drop(server);
}

/// A server that has served a client and is waiting for the next one stops
/// promptly when dropped.
#[test]
fn test_server_waiting_for_its_next_client_stops_when_dropped() {
    let name = pipe_name("waiting");
    let (tx, rx) = mpsc::channel();
    let server = PipeServer::start(&name, move |m| {
        let _ = tx.send(m);
    })
    .expect("server starts");
    PipeClient::new(name).send(&IpcMessage::Show).expect("send");
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(10)),
        Ok(IpcMessage::Show)
    );
    // The server loops back to waiting for a client.
    std::thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    drop(server);
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// DPAPI blobs are limited to 4 GiB: that length is accepted and one byte more
/// is refused instead of being truncated.
#[test]
fn test_blob_length_limit() {
    assert_eq!(blob_len(0).unwrap(), 0);
    assert_eq!(blob_len(u32::MAX as usize).unwrap(), u32::MAX);
    let err = blob_len(u32::MAX as usize + 1).unwrap_err();
    assert!(err.to_string().contains("too large"), "{err}");
}
