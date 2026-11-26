//! DPAPI round trip (Windows only).
#![cfg(windows)]
#![allow(clippy::unwrap_used)]

use tokenmon_platform::secrets::Encryptor;

#[cfg(windows)]
#[test]
fn dpapi_round_trip() {
    use tokenmon_platform_windows::secrets::DpapiEncryptor;
    let e = DpapiEncryptor;
    let blob = e.protect(b"hello").unwrap();
    assert_ne!(blob, b"hello");
    assert_eq!(e.unprotect(&blob).unwrap().as_slice(), b"hello");
    assert!(e.unprotect(b"garbage").is_err());
}
