//! Downloader failure paths: transport errors, unwritable destinations,
//! followed and runaway redirects.

use tokenmon_core::update::{InstallError, sha256_hex};
use tokenmon_platform::install::{Downloader, HttpDownloader, TrustPolicy};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn dl() -> HttpDownloader {
    HttpDownloader::new(TrustPolicy::AnyHostForTests, "TokenMon/test").expect("client builds")
}

fn message(e: InstallError) -> String {
    match e {
        InstallError::Download(m) => m,
        other => panic!("expected a download error, got {other:?}"),
    }
}

/// A refused connection is a download error that never leaks the URL.
#[tokio::test]
async fn test_connection_failure_is_a_download_error_without_the_url() {
    let dir = tempfile::tempdir().expect("tempdir");
    let e = dl()
        .download(
            "http://127.0.0.1:1/secret-name.zip?token=abc",
            &dir.path().join("x"),
            1024,
        )
        .await
        .expect_err("nothing listens on port 1");
    let msg = message(e);
    assert!(
        !msg.contains("token=abc") && !msg.contains("secret-name"),
        "{msg}"
    );
}

/// A destination that cannot be created is reported, not panicked on.
#[tokio::test]
async fn test_unwritable_destination_is_reported() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"x".to_vec()))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let dest = dir.path().join("no-such-dir").join("a.zip");
    let e = dl()
        .download(&server.uri(), &dest, 1024)
        .await
        .expect_err("cannot create the file");
    assert_ne!(message(e), "");
}

/// A redirect to an allowed URL is followed and the final body is hashed.
#[tokio::test]
async fn test_redirect_is_followed_when_trusted() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/start"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/final"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/final"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"payload".to_vec()))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let dest = dir.path().join("a.bin");
    let got = dl()
        .download(&format!("{}/start", server.uri()), &dest, 1024)
        .await
        .expect("follows the redirect");
    assert_eq!(got.sha256, sha256_hex(b"payload"));
}

/// A redirect loop stops after a bounded number of hops and reads as
/// "not available" rather than spinning.
#[tokio::test]
async fn test_redirect_loop_is_bounded() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/again"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let e = dl()
        .download(&server.uri(), &dir.path().join("x"), 1024)
        .await
        .expect_err("loop must end");
    assert_ne!(message(e), "");
    assert!(server.received_requests().await.expect("recorded").len() <= 8);
}
