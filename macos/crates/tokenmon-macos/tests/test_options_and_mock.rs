//! Command-line parsing and the loopback OpenRouter mock.

use std::io::{Read, Write};
use std::net::TcpStream;

use tokenmon_macos::mock::{KEY_BODY, MockOpenRouter};
use tokenmon_macos::options::{Options, OptionsError, StatusRenderer};
use tokenmon_macos::skia_status::Chrome;

fn parse(args: &[&str]) -> Result<Options, OptionsError> {
    let mut all = vec!["tokenmon-macos".to_owned()];
    all.extend(args.iter().map(ToString::to_string));
    Options::parse(&all)
}

/// No arguments gives the defaults.
#[test]
fn test_defaults() {
    assert_eq!(parse(&[]).expect("ok"), Options::default());
    assert_eq!(Options::default().renderer, StatusRenderer::Skia);
}

/// Every flag is understood.
#[test]
fn test_every_flag_parses() {
    let o = parse(&[
        "--print-info-plist",
        "--mock-openrouter",
        "--keep-open",
        "--render-status",
        "/tmp/x",
        "--open-panel-after",
        "1.5",
        "--quit-after",
        "9",
        "--chrome",
        "dark",
        "--renderer",
        "appkit",
        "--panel-tab",
        "claude",
    ])
    .expect("ok");
    assert!(o.print_info_plist && o.mock_openrouter && o.keep_open);
    assert_eq!(o.render_status, Some("/tmp/x".into()));
    assert_eq!((o.open_panel_after, o.quit_after), (Some(1.5), Some(9.0)));
    assert_eq!(o.force_chrome, Some(Chrome::Dark));
    assert_eq!(o.renderer, StatusRenderer::AppKit);
    assert_eq!(o.panel_tab.as_deref(), Some("claude"));
    assert_eq!(
        parse(&["--chrome", "light"]).expect("ok").force_chrome,
        Some(Chrome::Light)
    );
    assert_eq!(
        parse(&["--renderer", "skia"]).expect("ok").renderer,
        StatusRenderer::Skia
    );
}

/// Bad input is a typed error naming the flag.
#[test]
fn test_bad_input_is_rejected_with_the_flag_named() {
    for (args, needle) in [
        (&["--bogus"][..], "unknown option --bogus"),
        (&["--chrome"][..], "--chrome needs a value"),
        (&["--chrome", "sepia"][..], "--chrome: unknown value sepia"),
        (&["--renderer", "gl"][..], "--renderer: unknown value gl"),
        (
            &["--quit-after", "soon"][..],
            "--quit-after: expected seconds",
        ),
        (
            &["--quit-after", "-1"][..],
            "--quit-after: expected seconds",
        ),
    ] {
        let err = parse(args).expect_err("rejected");
        assert!(err.to_string().contains(needle), "{err}");
    }
}

fn get(port: u16, path: &str, auth: bool) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let header = if auth {
        "Authorization: Bearer sk-or-v1-test\r\n"
    } else {
        ""
    };
    write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n{header}\r\n").expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).expect("read");
    out
}

/// The mock serves `/key`, 404s elsewhere, counts requests and notes a bearer
/// header without recording its value.
#[test]
fn test_mock_serves_key_counts_requests_and_notes_bearer() {
    let mock = MockOpenRouter::start().expect("start");
    let port: u16 = mock
        .base_url()
        .rsplit(':')
        .next()
        .and_then(|t| t.split('/').next())
        .and_then(|p| p.parse().ok())
        .expect("port");
    assert!(mock.base_url().starts_with("http://127.0.0.1:"));
    assert!(!mock.saw_bearer());
    let missing = get(port, "/api/v1/credits", false);
    assert!(missing.starts_with("HTTP/1.1 404"));
    assert!(!mock.saw_bearer());
    let key = get(port, "/api/v1/key", true);
    assert!(key.starts_with("HTTP/1.1 200") && key.ends_with(KEY_BODY));
    assert!(mock.saw_bearer());
    assert_eq!(mock.requests(), 2);
}
