//! OpenCode's sign-in remembers the workspace of the page the user lands on,
//! and the failure messages cover every host error.

use std::sync::Arc;
use std::time::Duration;

use tokenmon_app::sessions::{HostBrowserCleaner, Sessions};
use tokenmon_app::signin::{SignInService, WEBVIEW2_DOWNLOAD_URL, sign_in_failure_message};
use tokenmon_core::{
    AuthSession, BrowserStateCleaner, Clock, ProviderAuthSession, ProviderId, SystemClock,
};
use tokenmon_platform::secrets::{FakeEncryptor, Vault};
use tokenmon_platform::webauth::{
    FakeWebAuthHost, Input, ViewKind, WebAuthError, WebAuthHost, WebCookie,
};

/// Landing on a console workspace page stores its id on the OpenCode session
/// before the cookies are adopted and sign-in completes.
#[test]
fn test_opencode_sign_in_remembers_the_workspace_id_of_the_return_page() {
    let dir = tempfile::tempdir().expect("tempdir");
    let host = Arc::new(FakeWebAuthHost::default());
    let vault = Arc::new(Vault::new(dir.path(), Arc::new(FakeEncryptor::default())));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let cleaner: Arc<dyn BrowserStateCleaner> = Arc::new(HostBrowserCleaner::new(
        Arc::clone(&host) as Arc<dyn WebAuthHost>,
    ));
    let sessions = Arc::new(Sessions::open(dir.path(), &vault, &clock, &cleaner));
    let service = SignInService::new(
        Arc::clone(&host) as Arc<dyn WebAuthHost>,
        Arc::clone(&sessions),
        Arc::new(|_| {}),
    );
    service.start(ProviderId::OpenCode).expect("opens");
    let window = host
        .sessions
        .lock()
        .expect("lock")
        .get("opencode")
        .cloned()
        .expect("window");
    {
        let mut w = window.lock().expect("lock");
        w.set_cookies(vec![WebCookie::new("auth", "token", ".opencode.ai")]);
        for url in [
            "https://opencode.ai/auth/authorize",
            "https://opencode.ai/console/wrk_abc123",
        ] {
            w.send(Input::NavigationFinished {
                view: ViewKind::Main,
                url: url.to_owned(),
            });
        }
        w.advance(Duration::from_secs(5));
    }
    assert_eq!(
        sessions.opencode.workspace_id().as_deref(),
        Some("wrk_abc123")
    );
    assert!(sessions.opencode.state().is_signed_in);
}

/// Each way the sign-in window can fail to open has its own user message.
#[test]
fn test_failure_messages_cover_every_host_error() {
    let missing = sign_in_failure_message(&WebAuthError::BrowserUnavailable("boom".into()));
    assert!(missing.contains(WEBVIEW2_DOWNLOAD_URL));
    assert_eq!(
        sign_in_failure_message(&WebAuthError::Unsupported),
        "Sign-in is only available on Windows."
    );
    assert_eq!(
        sign_in_failure_message(&WebAuthError::InvalidConfig("bad url".into())),
        "Sign-in could not open: invalid sign-in configuration (bad url)."
    );
}
