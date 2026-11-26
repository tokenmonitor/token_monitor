//! Decision #38 for OpenRouter: a key the vault refuses is not saved, the
//! typed vault message is surfaced and nothing reaches the disk (upstream #57).

use std::sync::Arc;

use tokenmon_app::sessions::{HostBrowserCleaner, Sessions};
use tokenmon_core::{
    AuthSession, BrowserStateCleaner, Clock, SystemClock, VAULT_UNAVAILABLE_MESSAGE,
};
use tokenmon_platform::secrets::{FakeEncryptor, Vault};
use tokenmon_platform::webauth::{FakeWebAuthHost, WebAuthHost};

const KEY: &str = "sk-or-v1-PLAINTEXT-MARKER";

#[test]
fn a_refused_openrouter_key_is_not_saved_and_the_error_is_surfaced() {
    let dir = tempfile::tempdir().expect("tempdir");
    let enc = Arc::new(FakeEncryptor::default());
    enc.set_fail_protect(true);
    let host = Arc::new(FakeWebAuthHost::default());
    let vault = Arc::new(Vault::new(dir.path(), enc));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let cleaner: Arc<dyn BrowserStateCleaner> = Arc::new(HostBrowserCleaner::new(
        Arc::clone(&host) as Arc<dyn WebAuthHost>,
    ));
    let sessions = Sessions::open(dir.path(), &vault, &clock, &cleaner);

    assert!(sessions.openrouter.save_api_key(KEY).is_err());

    let state = sessions.openrouter.state();
    assert!(!state.is_signed_in);
    assert!(sessions.openrouter.api_key().is_none());
    assert_eq!(
        state.last_auth_error.as_deref(),
        Some(VAULT_UNAVAILABLE_MESSAGE)
    );
    for entry in std::fs::read_dir(dir.path()).expect("dir") {
        let bytes = std::fs::read(entry.expect("entry").path()).expect("read");
        assert!(
            !String::from_utf8_lossy(&bytes).contains(KEY),
            "the key reached the disk"
        );
    }
}
