//! Every step of the install flow can fail, and each failure must surface as
//! a typed error that leaves the installed exe untouched with no staging
//! files behind. A scripted verifier, version reader and downloader inject the
//! failure at exactly one step.
#![allow(clippy::unwrap_used)]

mod common;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{fake_msi, fake_pe, make_zip, sha};
use tokenmon_core::BoxFuture;
use tokenmon_core::update::{
    AppVersion, AssetKind, InstallError, InstallOutcome, InstallRequest, ReleaseAsset,
    ReleaseSource, UpdateInstaller,
};
use tokenmon_platform::install::{DownloadedFile, Downloader};
use tokenmon_platform_windows::install::{
    CommandSpec, PackageVersionReader, ProcessLauncher, SignatureVerifier, WindowsUpdateInstaller,
};

const ZIP_URL: &str = "https://github.com/tokenmonitor/token_monitor/releases/download/windows-v1.6.0/tokenmon-windows-x64.zip";
const MSI_URL: &str = "https://github.com/tokenmonitor/token_monitor/releases/download/windows-v1.6.0/tokenmon-windows-x64.msi";

/// What a scripted step does on its n-th call (1-based).
type Step<T> = Box<dyn Fn(usize, &Path) -> Result<T, InstallError> + Send + Sync>;

struct Verifier {
    exe_calls: AtomicUsize,
    msi_calls: AtomicUsize,
    exe: Step<()>,
    msi: Step<()>,
}

impl SignatureVerifier for Verifier {
    fn verify_exe(&self, path: &Path) -> Result<(), InstallError> {
        let n = self.exe_calls.fetch_add(1, Ordering::SeqCst) + 1;
        (self.exe)(n, path)
    }
    fn verify_msi(&self, path: &Path) -> Result<(), InstallError> {
        let n = self.msi_calls.fetch_add(1, Ordering::SeqCst) + 1;
        (self.msi)(n, path)
    }
}

fn accept() -> Step<()> {
    Box::new(|_, _| Ok(()))
}

struct Versions {
    exe_calls: AtomicUsize,
    exe: Step<AppVersion>,
    msi: Step<AppVersion>,
}

fn v(s: &str) -> AppVersion {
    AppVersion::parse(s).unwrap()
}

impl PackageVersionReader for Versions {
    fn exe_version(&self, path: &Path) -> Result<AppVersion, InstallError> {
        let n = self.exe_calls.fetch_add(1, Ordering::SeqCst) + 1;
        (self.exe)(n, path)
    }
    fn msi_version(&self, path: &Path) -> Result<AppVersion, InstallError> {
        (self.msi)(1, path)
    }
}

/// Serves `body`, or fails the download with `error`.
struct Serve {
    body: Vec<u8>,
    error: Option<InstallError>,
}

impl Downloader for Serve {
    fn download<'a>(
        &'a self,
        _url: &'a str,
        dest: &'a Path,
        _max: u64,
    ) -> BoxFuture<'a, Result<DownloadedFile, InstallError>> {
        Box::pin(async move {
            if let Some(e) = &self.error {
                return Err(e.clone());
            }
            fs::write(dest, &self.body).unwrap();
            Ok(DownloadedFile {
                path: dest.to_path_buf(),
                size: self.body.len() as u64,
                sha256: sha(&self.body),
            })
        })
    }
}

struct Launcher {
    fail: bool,
}

impl ProcessLauncher for Launcher {
    fn spawn_detached(&self, _spec: &CommandSpec) -> io::Result<()> {
        if self.fail {
            Err(io::Error::other("no msiexec"))
        } else {
            Ok(())
        }
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    exe: PathBuf,
    work: PathBuf,
    installer: WindowsUpdateInstaller,
}

fn rig(downloader: Serve, verifier: Verifier, versions: Versions, launcher_fails: bool) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let (app, work) = (dir.path().join("app"), dir.path().join("work"));
    fs::create_dir_all(&app).unwrap();
    fs::create_dir_all(&work).unwrap();
    let exe = app.join("TokenMon.exe");
    fs::write(&exe, fake_pe("old")).unwrap();
    let installer = WindowsUpdateInstaller::new(
        ReleaseSource::new("windows-v", "tokenmon-windows-"),
        exe.clone(),
        4242,
        work.clone(),
        Arc::new(downloader),
        Arc::new(verifier),
        Arc::new(Launcher {
            fail: launcher_fails,
        }),
    )
    .with_version_reader(Arc::new(versions));
    Rig {
        _dir: dir,
        exe,
        work,
        installer,
    }
}

fn verifier(exe: Step<()>, msi: Step<()>) -> Verifier {
    Verifier {
        exe_calls: AtomicUsize::new(0),
        msi_calls: AtomicUsize::new(0),
        exe,
        msi,
    }
}

fn versions(exe: Step<AppVersion>, msi: Step<AppVersion>) -> Versions {
    Versions {
        exe_calls: AtomicUsize::new(0),
        exe,
        msi,
    }
}

fn announced() -> Step<AppVersion> {
    Box::new(|_, _| Ok(v("1.6.0")))
}

fn zip_body() -> Vec<u8> {
    make_zip(&[("TokenMon.exe", &fake_pe("new"))])
}

fn serve(body: Vec<u8>) -> Serve {
    Serve { body, error: None }
}

fn request(body: &[u8], kind: AssetKind, url: &str) -> InstallRequest {
    InstallRequest {
        asset: ReleaseAsset {
            name: "x".into(),
            url: url.into(),
            sha256: Some(sha(body)),
        },
        kind,
        expected_sha256: sha(body),
        current: v("1.5.0"),
        target: v("1.6.0"),
    }
}

async fn install(r: &Rig, body: &[u8], kind: AssetKind) -> Result<InstallOutcome, InstallError> {
    let url = if kind == AssetKind::Zip {
        ZIP_URL
    } else {
        MSI_URL
    };
    r.installer.install(request(body, kind, url)).await
}

fn assert_no_leftovers(r: &Rig) {
    let siblings: Vec<_> = fs::read_dir(r.exe.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(siblings, ["TokenMon.exe"], "no staged or aside files");
    assert_eq!(fs::read_dir(&r.work).unwrap().count(), 0, "work dir clean");
}

fn assert_untouched(r: &Rig) {
    assert_eq!(fs::read(&r.exe).unwrap(), fake_pe("old"));
    assert_no_leftovers(r);
}

/// A failed download (zip or MSI) surfaces the downloader's own error.
#[tokio::test]
async fn test_download_error_is_surfaced_for_both_package_kinds() {
    for (kind, body) in [(AssetKind::Zip, zip_body()), (AssetKind::Msi, fake_msi())] {
        let r = rig(
            Serve {
                body: body.clone(),
                error: Some(InstallError::Download("offline".into())),
            },
            verifier(accept(), accept()),
            versions(announced(), announced()),
            false,
        );
        let err = install(&r, &body, kind).await.unwrap_err();
        assert!(
            matches!(&err, InstallError::Download(m) if m == "offline"),
            "{err:?}"
        );
        assert_untouched(&r);
    }
}

/// A download whose checksum matches but that is not a zip archive fails at
/// extraction.
#[tokio::test]
async fn test_zip_that_is_not_an_archive_is_refused() {
    let body = b"this is not a zip".to_vec();
    let r = rig(
        serve(body.clone()),
        verifier(accept(), accept()),
        versions(announced(), announced()),
        false,
    );
    assert!(install(&r, &body, AssetKind::Zip).await.is_err());
    assert_untouched(&r);
}

/// An exe whose version resource cannot be read is refused before staging.
#[tokio::test]
async fn test_unreadable_exe_version_is_refused() {
    let body = zip_body();
    let r = rig(
        serve(body.clone()),
        verifier(accept(), accept()),
        versions(
            Box::new(|_, _| Err(InstallError::Verification("no version resource".into()))),
            announced(),
        ),
        false,
    );
    let err = install(&r, &body, AssetKind::Zip).await.unwrap_err();
    assert!(
        matches!(&err, InstallError::Verification(m) if m.contains("no version")),
        "{err:?}"
    );
    assert_untouched(&r);
}

/// The staged copy is checked again: refusing it (signature or version)
/// removes the staged file and leaves the installed exe alone.
#[tokio::test]
async fn test_staged_copy_failing_its_checks_is_refused_and_removed() {
    let body = zip_body();
    let bad_staged_signature = rig(
        serve(body.clone()),
        verifier(
            Box::new(|n, _| {
                if n == 2 {
                    Err(InstallError::Verification("staged copy untrusted".into()))
                } else {
                    Ok(())
                }
            }),
            accept(),
        ),
        versions(announced(), announced()),
        false,
    );
    let err = install(&bad_staged_signature, &body, AssetKind::Zip)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, InstallError::Verification(m) if m.contains("staged")),
        "{err:?}"
    );
    assert_untouched(&bad_staged_signature);

    let bad_staged_version = rig(
        serve(body.clone()),
        verifier(accept(), accept()),
        versions(
            Box::new(|n, _| {
                if n == 2 {
                    Ok(v("1.4.0"))
                } else {
                    Ok(v("1.6.0"))
                }
            }),
            announced(),
        ),
        false,
    );
    let err = install(&bad_staged_version, &body, AssetKind::Zip)
        .await
        .unwrap_err();
    assert!(matches!(err, InstallError::Verification(_)), "{err:?}");
    assert_untouched(&bad_staged_version);
}

/// If the unpacked exe disappears before it is staged the install fails with
/// an install error instead of a panic.
#[tokio::test]
async fn test_exe_vanishing_before_staging_is_an_install_error() {
    let body = zip_body();
    let r = rig(
        serve(body.clone()),
        verifier(
            Box::new(|n, path| {
                if n == 1 {
                    fs::remove_file(path).unwrap();
                }
                Ok(())
            }),
            accept(),
        ),
        versions(announced(), announced()),
        false,
    );
    let err = install(&r, &body, AssetKind::Zip).await.unwrap_err();
    assert!(matches!(err, InstallError::Install(_)), "{err:?}");
    assert_untouched(&r);
}

/// If the running exe disappears between the staged checks and the swap, the
/// swap fails cleanly and the staged copy is removed.
#[tokio::test]
async fn test_running_exe_vanishing_before_the_swap_fails_cleanly() {
    let body = zip_body();
    let r = rig(
        serve(body.clone()),
        verifier(
            Box::new({
                let running = Arc::new(std::sync::OnceLock::<PathBuf>::new());
                move |n, path| {
                    if n == 2 {
                        // The staged copy sits beside the running exe.
                        let running =
                            running.get_or_init(|| path.parent().unwrap().join("TokenMon.exe"));
                        fs::remove_file(running).unwrap();
                    }
                    Ok(())
                }
            }),
            accept(),
        ),
        versions(announced(), announced()),
        false,
    );
    let err = install(&r, &body, AssetKind::Zip).await.unwrap_err();
    assert!(matches!(err, InstallError::Install(_)), "{err:?}");
    assert!(!r.exe.exists(), "nothing was installed in its place");
    assert_eq!(fs::read_dir(r.exe.parent().unwrap()).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&r.work).unwrap().count(), 0);
}

/// An MSI whose product version cannot be read is refused and its folder
/// removed.
#[tokio::test]
async fn test_unreadable_msi_version_is_refused() {
    let body = fake_msi();
    let r = rig(
        serve(body.clone()),
        verifier(accept(), accept()),
        versions(
            announced(),
            Box::new(|_, _| Err(InstallError::Verification("no property table".into()))),
        ),
        false,
    );
    let err = install(&r, &body, AssetKind::Msi).await.unwrap_err();
    assert!(matches!(err, InstallError::Verification(_)), "{err:?}");
    assert_untouched(&r);
}

/// A package that vanishes after verification cannot be re-hashed, so msiexec
/// is not started and the install fails with an install error.
#[tokio::test]
async fn test_msi_vanishing_after_verification_is_an_install_error() {
    let body = fake_msi();
    let r = rig(
        serve(body.clone()),
        verifier(
            accept(),
            Box::new(|_, path| {
                fs::remove_file(path).unwrap();
                Ok(())
            }),
        ),
        versions(announced(), announced()),
        false,
    );
    // The version reader never opens the file, so the vanish is only noticed
    // by the verify-to-run digest check.
    let err = install(&r, &body, AssetKind::Msi).await.unwrap_err();
    assert!(matches!(err, InstallError::Install(_)), "{err:?}");
    assert_untouched(&r);
}

/// The relaunch failing after a good zip swap is not an error: the update is
/// installed and the user is asked to restart by hand.
#[tokio::test]
async fn test_zip_swap_with_failing_relaunch_asks_for_manual_restart() {
    let body = zip_body();
    let r = rig(
        serve(body.clone()),
        verifier(accept(), accept()),
        versions(announced(), announced()),
        true,
    );
    let out = install(&r, &body, AssetKind::Zip).await.unwrap();
    assert_eq!(out, InstallOutcome::InstalledNeedsManualRestart);
    assert_eq!(fs::read(&r.exe).unwrap(), fake_pe("new"));
}
