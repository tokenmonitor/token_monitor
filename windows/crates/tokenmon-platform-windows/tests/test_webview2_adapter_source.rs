//! Source-level guard on the WebView2 adapter: it never overrides the user
//! agent or exposes host objects or web messaging to provider pages.

// testSignInBrowserDoesNotOverrideTheUserAgent (Windows: the adapter never
// sets a user agent, so WebView2's default Edge UA is used).
#[test]
fn adapter_never_overrides_the_user_agent_or_exposes_host_objects() {
    let src = include_str!("../src/webauth/webview2.rs");
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!code.contains("UserAgent"), "must not set a user agent");
    assert!(!code.contains("AddHostObjectToScript"), "no host objects");
    assert!(code.contains("SetAreHostObjectsAllowed(false)"));
    assert!(code.contains("SetIsWebMessageEnabled(false)"));
}
