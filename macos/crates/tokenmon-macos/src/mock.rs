//! A loopback stand-in for the OpenRouter API, used to prove the polling
//! pipeline end to end without a real credential (`--mock-openrouter`).
//!
//! It answers `GET .../key` with a fixed key record and counts requests. It
//! records only *whether* a bearer token arrived, never its value.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

/// The `/key` body: a 25 USD monthly limit with 12.5 USD left, so the headline
/// used percentage is 50.
pub const KEY_BODY: &str = r#"{"data":{"label":"mock key","usage":12.5,"usage_daily":1.2,"usage_weekly":6.4,"usage_monthly":12.5,"limit":25.0,"limit_remaining":12.5,"limit_reset":"monthly","is_free_tier":false,"is_management_key":false}}"#;

/// A running mock server. Dropping it stops the accept loop.
pub struct MockOpenRouter {
    addr: SocketAddr,
    requests: Arc<AtomicUsize>,
    saw_bearer: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockOpenRouter {
    /// Binds a free loopback port and serves until dropped.
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let requests = Arc::new(AtomicUsize::new(0));
        let saw_bearer = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (r, b, s) = (
            Arc::clone(&requests),
            Arc::clone(&saw_bearer),
            Arc::clone(&stop),
        );
        let thread = std::thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => handle(stream, &r, &b),
                    Err(_) => std::thread::sleep(Duration::from_millis(20)),
                }
            }
        });
        Ok(Self {
            addr,
            requests,
            saw_bearer,
            stop,
            thread: Some(thread),
        })
    }

    /// Base URL to hand to the OpenRouter client.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/api/v1", self.addr.port())
    }

    /// How many requests were answered.
    pub fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    /// Whether any request carried an `Authorization: Bearer` header.
    pub fn saw_bearer(&self) -> bool {
        self.saw_bearer.load(Ordering::SeqCst)
    }
}

impl Drop for MockOpenRouter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn handle(mut stream: TcpStream, requests: &AtomicUsize, saw_bearer: &AtomicBool) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut buf = [0_u8; 4096];
    let n = stream.read(&mut buf).unwrap_or(0);
    let head = String::from_utf8_lossy(&buf[..n]).to_lowercase();
    requests.fetch_add(1, Ordering::SeqCst);
    if head.contains("authorization: bearer ") {
        saw_bearer.store(true, Ordering::SeqCst);
    }
    let (status, body) = if head.starts_with("get /api/v1/key ") {
        ("200 OK", KEY_BODY)
    } else {
        ("404 Not Found", r#"{"error":"not found"}"#)
    };
    let response = format!(
        "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
}
