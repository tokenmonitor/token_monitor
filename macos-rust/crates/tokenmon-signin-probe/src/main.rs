//! Sign-in probe for issue #89 open question 4 (wry vs objc2-web-kit).
//!
//! Serves a local page that sets an HttpOnly cookie, loads it in a
//! non-persistent web view and reads the cookie back, then reports:
//!  1. is the HttpOnly cookie returned (and invisible to `document.cookie`)?
//!  2. is the store non-persistent (a later process does not send the cookie)?
//!  3. are navigation events exposed?
//!
//! Usage: `signin-probe wry|webkit set|check [--persistent]`
//! `set` loads the page (cookie with Max-Age) and reads it back; `check` runs a
//! fresh process against a server that sets nothing and reports whether the
//! earlier cookie came along. `--persistent` uses the default store, to prove
//! the check can fail.

#[cfg(target_os = "macos")]
mod server;
#[cfg(target_os = "macos")]
mod webkit;
#[cfg(target_os = "macos")]
mod wry_probe;

#[cfg(target_os = "macos")]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (engine, phase) = (
        args.get(1).map_or("", String::as_str),
        args.get(2).map_or("", String::as_str),
    );
    let persistent = args.iter().any(|a| a == "--persistent");
    let sets_cookie = phase == "set";
    let server = server::Server::start(sets_cookie);
    let url = server.url();
    match engine {
        "wry" => wry_probe::run(&url, persistent, &server),
        "webkit" => webkit::run(&url, persistent, &server),
        _ => eprintln!("usage: signin-probe wry|webkit set|check [--persistent]"),
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("signin-probe is a macOS tool");
}
