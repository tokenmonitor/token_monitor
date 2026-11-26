//! Hermetic tests for the AppUserModelID registration over an in-memory registry.

use std::sync::Arc;

use tokenmon_platform::shell::{MemoryRegistry, Registry};
use tokenmon_platform_windows::notify::{AumidRegistration, AumidStatus};

/// First run registers the AUMID; the second run writes nothing.
#[test]
fn test_aumid_registration_is_idempotent() {
    let reg = Arc::new(MemoryRegistry::new());
    let r = AumidRegistration::new(reg.clone(), Some(r"C:\TokenMon\icon.ico".into()));
    assert_eq!(
        r.ensure().expect("operation succeeds"),
        AumidStatus::Registered
    );
    let key = AumidRegistration::key();
    assert_eq!(key, r"Software\Classes\AppUserModelId\TokenMon");
    assert_eq!(
        reg.get_string(&key, "DisplayName")
            .expect("operation succeeds")
            .as_deref(),
        Some("TokenMon")
    );
    assert_eq!(
        reg.get_string(&key, "IconUri")
            .expect("operation succeeds")
            .as_deref(),
        Some(r"C:\TokenMon\icon.ico")
    );
    reg.set_fail_writes(true);
    assert_eq!(
        r.ensure().expect("operation succeeds"),
        AumidStatus::AlreadyRegistered
    );
}

/// A moved install (new icon path) re-registers; removal is idempotent.
#[test]
fn test_aumid_updates_stale_icon_and_removes() {
    let reg = Arc::new(MemoryRegistry::new());
    AumidRegistration::new(reg.clone(), Some("a.ico".into()))
        .ensure()
        .expect("operation succeeds");
    let r = AumidRegistration::new(reg.clone(), Some("b.ico".into()));
    assert_eq!(
        r.ensure().expect("operation succeeds"),
        AumidStatus::Registered
    );
    r.remove().expect("operation succeeds");
    r.remove().expect("operation succeeds");
    assert!(
        !reg.key_exists(&AumidRegistration::key())
            .expect("operation succeeds")
    );
}

/// Registry write failures surface as typed errors.
#[test]
fn test_aumid_write_failure_is_reported() {
    let reg = Arc::new(MemoryRegistry::new());
    reg.set_fail_writes(true);
    assert!(AumidRegistration::new(reg, None).ensure().is_err());
}
