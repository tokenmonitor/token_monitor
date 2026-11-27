//! Manually entered credentials: precedence, trimming and never leaking the key.

use std::io::{Error, ErrorKind};
use std::path::Path;

use tokenmon_macos::credentials::{
    CredentialError, KeySource, LoadedKey, OPENROUTER_KEY_ENV, OPENROUTER_KEY_FILE_ENV,
    load_openrouter_key,
};

fn no_file(_: &Path) -> Result<String, Error> {
    Err(Error::new(ErrorKind::NotFound, "no file"))
}

/// Without either variable there is simply no key (not an error).
#[test]
fn test_no_variables_means_no_key() {
    let got = load_openrouter_key(&|_| None, &no_file).expect("not an error");
    assert!(got.is_none());
}

/// The variable wins over the file and is trimmed.
#[test]
fn test_variable_wins_over_file_and_is_trimmed() {
    let env = |n: &str| match n {
        OPENROUTER_KEY_ENV => Some("  sk-or-v1-a\n".to_owned()),
        OPENROUTER_KEY_FILE_ENV => Some("/nope".to_owned()),
        _ => None,
    };
    let got = load_openrouter_key(&env, &no_file)
        .expect("ok")
        .expect("key");
    assert_eq!(got.expose(), "sk-or-v1-a");
    assert_eq!(got.source, KeySource::Env);
}

/// A key file is read and trimmed; its path is the recorded source.
#[test]
fn test_file_key_is_read_and_trimmed() {
    let env = |n: &str| (n == OPENROUTER_KEY_FILE_ENV).then(|| "/tmp/k".to_owned());
    let read = |p: &Path| {
        assert_eq!(p, Path::new("/tmp/k"));
        Ok("sk-or-v1-b\n".to_owned())
    };
    let got = load_openrouter_key(&env, &read).expect("ok").expect("key");
    assert_eq!(got.expose(), "sk-or-v1-b");
    assert_eq!(got.source, KeySource::File("/tmp/k".into()));
}

/// An unreadable file names the path but never anything secret.
#[test]
fn test_unreadable_file_is_a_typed_error() {
    let env = |n: &str| (n == OPENROUTER_KEY_FILE_ENV).then(|| "/tmp/k".to_owned());
    let err = load_openrouter_key(&env, &no_file).expect_err("unreadable");
    assert_eq!(err, CredentialError::UnreadableFile("/tmp/k".into()));
    assert_eq!(err.to_string(), "cannot read the key file /tmp/k");
}

/// Blank values are refused in both places, with the origin named.
#[test]
fn test_blank_values_are_refused() {
    let env = |n: &str| (n == OPENROUTER_KEY_ENV).then(|| "   ".to_owned());
    let err = load_openrouter_key(&env, &no_file).expect_err("blank");
    assert_eq!(err, CredentialError::Empty(OPENROUTER_KEY_ENV.into()));
    assert!(err.to_string().contains("empty"));

    let env = |n: &str| (n == OPENROUTER_KEY_FILE_ENV).then(|| "/tmp/k".to_owned());
    let err = load_openrouter_key(&env, &|_| Ok("\n".to_owned())).expect_err("blank file");
    assert_eq!(err, CredentialError::Empty("/tmp/k".into()));
}

/// An empty file-path variable counts as unset.
#[test]
fn test_empty_path_variable_is_unset() {
    let env = |n: &str| (n == OPENROUTER_KEY_FILE_ENV).then(String::new);
    assert!(load_openrouter_key(&env, &no_file).expect("ok").is_none());
}

/// `Debug` never prints the key.
#[test]
fn test_debug_output_redacts_the_key() {
    let key = LoadedKey::mock("sk-or-v1-supersecret");
    let shown = format!("{key:?}");
    assert!(!shown.contains("supersecret"));
    assert!(shown.contains("redacted"));
    assert_eq!(key.source, KeySource::Mock);
}
