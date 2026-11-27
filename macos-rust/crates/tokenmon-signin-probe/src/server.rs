//! A loopback server: `/` sets an HttpOnly cookie (in `set` mode) and shows what
//! JavaScript can see; `/next` is a second page to trigger a navigation event.
//! Records whether a request arrived carrying the cookie.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Fixed port: cookies are per host, so two processes share them via 127.0.0.1.
const PORT: u16 = 17891;
/// The cookie's value (a throwaway, not a secret).
pub const COOKIE_VALUE: &str = "probe-session-value";

/// The running server.
#[derive(Clone)]
pub struct Server {
    saw_cookie: Arc<AtomicBool>,
}

impl Server {
    /// Starts serving in a thread.
    pub fn start(sets_cookie: bool) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", PORT)).expect("port 17891 is free");
        let saw_cookie = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&saw_cookie);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buf = [0_u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                if head.contains(COOKIE_VALUE) {
                    flag.store(true, Ordering::SeqCst);
                }
                let cookie = if sets_cookie && head.starts_with("GET / ") {
                    format!("set-cookie: tm_session={COOKIE_VALUE}; HttpOnly; Path=/; Max-Age=3600\r\n")
                } else {
                    String::new()
                };
                let body = if head.starts_with("GET /next") {
                    "<title>next</title>second page".to_owned()
                } else {
                    "<title>probe</title><script>document.title='js-sees:['+document.cookie+']'</script><a href=/next>next</a>".to_owned()
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\n{cookie}content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        Self { saw_cookie }
    }

    /// The start URL.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{PORT}/")
    }

    /// Whether any request carried the cookie so far.
    pub fn saw_cookie(&self) -> bool {
        self.saw_cookie.load(Ordering::SeqCst)
    }
}
