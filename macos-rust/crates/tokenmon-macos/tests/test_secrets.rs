//! The per-process vault encryption.

use tokenmon_macos::secrets::EphemeralEncryptor;
use tokenmon_platform::secrets::Encryptor;

/// Blobs of any length round-trip and never equal the plaintext.
#[test]
fn test_protect_then_unprotect_round_trips_for_many_lengths() {
    let enc = EphemeralEncryptor::with_key([7; 32]);
    for len in [0_usize, 1, 31, 32, 33, 100, 1000] {
        let plain: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        let sealed = enc.protect(&plain).expect("protect");
        if len > 4 {
            assert!(!sealed.windows(len).any(|w| w == plain.as_slice()));
        }
        assert_eq!(&**enc.unprotect(&sealed).expect("unprotect"), &plain);
    }
}

/// A blob sealed with another key does not decrypt to the plaintext, and a
/// blob without the magic prefix is rejected.
#[test]
fn test_other_key_and_garbage_do_not_open() {
    let (a, b) = (
        EphemeralEncryptor::with_key([1; 32]),
        EphemeralEncryptor::with_key([2; 32]),
    );
    let sealed = a.protect(b"sk-or-v1-secret").expect("protect");
    assert_ne!(
        &**b.unprotect(&sealed).expect("decodes to noise"),
        b"sk-or-v1-secret"
    );
    assert!(a.unprotect(b"plain bytes").is_err());
}

/// Random keys differ between encryptors (reads `/dev/urandom`, so Unix only).
#[cfg(unix)]
#[test]
fn test_random_keys_differ() {
    let one = EphemeralEncryptor::random().expect("entropy");
    let two = EphemeralEncryptor::random().expect("entropy");
    let sealed = one.protect(b"hello world").expect("protect");
    assert_ne!(&**two.unprotect(&sealed).expect("noise"), b"hello world");
}
