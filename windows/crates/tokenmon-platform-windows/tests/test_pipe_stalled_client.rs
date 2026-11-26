//! The real named pipe: a stalled client cannot hold the single-instance server.
#![cfg(windows)]
#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use tokenmon_platform::shell::IpcMessage;

/// Real pipe: a client that connects and stalls must not stop the next client,
/// and dropping the server must not hang. Runs on the Windows CI.
#[test]
fn windows_pipe_survives_a_stalled_client_and_drops_promptly() {
    use std::fs::OpenOptions;
    use std::sync::mpsc;
    use tokenmon_platform::shell::ipc::IpcClient;
    use tokenmon_platform_windows::shell::windows::{
        PipeClient, PipeServer, current_user_pipe_name,
    };

    let name = format!(
        "{}.test{}",
        current_user_pipe_name().unwrap(),
        std::process::id()
    );
    let (tx, rx) = mpsc::channel();
    let server = PipeServer::start(&name, move |m| {
        let _ = tx.send(m);
    })
    .unwrap();

    // Squatting: a second server on the same name is refused.
    assert!(PipeServer::start(&name, |_| {}).is_err());

    // The stalled client: connected, writes nothing, never closes.
    let stalled = OpenOptions::new().write(true).open(&name).unwrap();

    // A well-behaved client still gets through once the deadline frees the pipe.
    PipeClient::new(name.clone())
        .send(&IpcMessage::Show)
        .unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(10)),
        Ok(IpcMessage::Show)
    );

    // A second stalled client, then quit: Drop must not wait for it.
    let stalled2 = OpenOptions::new().write(true).open(&name);
    let start = Instant::now();
    drop(server);
    assert!(start.elapsed() < Duration::from_secs(5));
    drop((stalled, stalled2));
}
