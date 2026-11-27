//! The OpenRouter base URL override accepts only loopback HTTP addresses.

use tokenmon_app::providers::openrouter_base_url;

const PRODUCTION: &str = "https://openrouter.ai/api/v1";

/// With no override the production URL is used.
#[test]
fn test_base_url_defaults_to_production() {
    assert_eq!(openrouter_base_url(None), PRODUCTION);
}

/// Loopback HTTP overrides (the macOS beta's mock server) are honoured.
#[test]
fn test_base_url_accepts_loopback_http() {
    assert_eq!(
        openrouter_base_url(Some("http://127.0.0.1:8123/api/v1")),
        "http://127.0.0.1:8123/api/v1"
    );
    assert_eq!(
        openrouter_base_url(Some("http://localhost:8123")),
        "http://localhost:8123"
    );
}

/// Any other host, or HTTPS, is ignored so a stray variable cannot redirect a real key.
#[test]
fn test_base_url_ignores_remote_overrides() {
    for url in [
        "https://evil.example/api/v1",
        "http://evil.example:80",
        "http://127.0.0.1.evil.example:80",
        "http://localhost:80@evil.example",
        "",
    ] {
        assert_eq!(openrouter_base_url(Some(url)), PRODUCTION, "{url}");
    }
}
