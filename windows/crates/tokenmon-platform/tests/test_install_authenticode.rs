//! Port of `AppInstallerTests` testUnsignedAppIsRefused and
//! testAppModifiedAfterSigningIsRefused: with `RequireAuthenticode` an unsigned
//! or tampered download is refused through the whole install flow, and the
//! installed exe is untouched with no siblings or staging files left behind.
#![allow(clippy::unwrap_used)]

mod common;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::{fake_pe, make_zip, sha};
use tokenmon_core::BoxFuture;
use tokenmon_core::update::{
    AppVersion, AssetKind, InstallError, InstallRequest, ReleaseAsset, ReleaseSource,
    UpdateInstaller,
};
use tokenmon_platform::install::verify::{SignaturePolicy, verifier_for_policy};
use tokenmon_platform::install::{
    CommandSpec, DownloadedFile, Downloader, ProcessLauncher, WindowsUpdateInstaller,
};

const URL: &str = "https://github.com/tokenmonitor/token_monitor/releases/download/windows-v1.6.0/tokenmon-windows-x64.zip";
#[cfg(windows)]
const NOT_TRUSTED: &str = "The download is not signed by a trusted publisher.";

struct FakeDownloader(Vec<u8>);

impl Downloader for FakeDownloader {
    fn download<'a>(
        &'a self,
        _url: &'a str,
        dest: &'a Path,
        _max: u64,
    ) -> BoxFuture<'a, Result<DownloadedFile, InstallError>> {
        Box::pin(async move {
            fs::write(dest, &self.0).unwrap();
            Ok(DownloadedFile {
                path: dest.to_path_buf(),
                size: self.0.len() as u64,
                sha256: sha(&self.0),
            })
        })
    }
}

struct NeverLaunches;

impl ProcessLauncher for NeverLaunches {
    fn spawn_detached(&self, _spec: &CommandSpec) -> io::Result<()> {
        panic!("a refused download must never launch anything");
    }
}

struct Rig {
    _dir: tempfile::TempDir,
    exe: PathBuf,
    work: PathBuf,
    installer: WindowsUpdateInstaller,
}

/// An installed `old` exe and an installer that requires Authenticode and is
/// served `zip`.
fn rig(zip: &[u8]) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let (app, work) = (dir.path().join("app"), dir.path().join("work"));
    fs::create_dir_all(&app).unwrap();
    fs::create_dir_all(&work).unwrap();
    let exe = app.join("TokenMon.exe");
    fs::write(&exe, fake_pe("old")).unwrap();
    let installer = WindowsUpdateInstaller::new(
        ReleaseSource::default(),
        exe.clone(),
        4242,
        work.clone(),
        Arc::new(FakeDownloader(zip.to_vec())),
        verifier_for_policy(SignaturePolicy::RequireAuthenticode),
        Arc::new(NeverLaunches),
    );
    Rig {
        _dir: dir,
        exe,
        work,
        installer,
    }
}

fn request(zip: &[u8]) -> InstallRequest {
    InstallRequest {
        asset: ReleaseAsset {
            name: "x".into(),
            url: URL.into(),
            sha256: Some(sha(zip)),
        },
        kind: AssetKind::Zip,
        expected_sha256: sha(zip).to_uppercase(),
        current: AppVersion::parse("1.5.0").unwrap(),
        target: AppVersion::parse("1.6.0").unwrap(),
    }
}

fn assert_untouched(r: &Rig) {
    assert_eq!(fs::read(&r.exe).unwrap(), fake_pe("old"));
    let siblings: Vec<_> = fs::read_dir(r.exe.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(siblings, ["TokenMon.exe"]);
    assert_eq!(
        fs::read_dir(&r.work).unwrap().count(),
        0,
        "work dir cleaned"
    );
}

/// An unsigned exe is refused.
#[tokio::test]
async fn test_unsigned_app_is_refused() {
    let zip = make_zip(&[("TokenMon.exe", &fake_pe("new"))]);
    let r = rig(&zip);

    let err = r.installer.install(request(&zip)).await.unwrap_err();

    assert!(matches!(err, InstallError::Verification(_)), "got {err:?}");
    #[cfg(windows)]
    assert_eq!(err, InstallError::Verification(NOT_TRUSTED.into()));
    assert_untouched(&r);
}

/// A genuinely signed exe (found on the host) that is changed after signing
/// is refused: its Authenticode hash no longer matches.
///
/// Needs an exe with an embedded Authenticode signature and revocation
/// checking; Git for Windows' `git.exe` is one (it is on the Windows CI
/// runners). Without one the test says so and returns, since the OS has no
/// other source of a signed binary this test may modify.
#[cfg(windows)]
#[tokio::test]
async fn test_app_modified_after_signing_is_refused() {
    let verifier = verifier_for_policy(SignaturePolicy::RequireAuthenticode);
    let Some(signed) = signed_exes()
        .into_iter()
        .find(|p| verifier.verify_exe(p).is_ok())
    else {
        eprintln!("skipped: no embedded-Authenticode-signed exe verifies on this host");
        return;
    };

    let mut tampered = fs::read(&signed).unwrap();
    // A byte in the DOS stub: inside the hashed image, outside the signature.
    tampered[0x60] ^= 0xff;
    let zip = make_zip(&[("TokenMon.exe", &tampered)]);
    let r = rig(&zip);

    let err = r.installer.install(request(&zip)).await.unwrap_err();

    assert_eq!(err, InstallError::Verification(NOT_TRUSTED.into()));
    assert_untouched(&r);
}

/// Exes that normally carry an embedded Authenticode signature.
#[cfg(windows)]
fn signed_exes() -> Vec<PathBuf> {
    let pf = |var: &str| std::env::var_os(var).map(PathBuf::from);
    let mut out = Vec::new();
    for root in ["ProgramFiles", "ProgramW6432"]
        .iter()
        .filter_map(|v| pf(v))
    {
        out.push(root.join(r"Git\cmd\git.exe"));
        out.push(root.join(r"Git\bin\bash.exe"));
        out.push(root.join(r"Git\mingw64\bin\git.exe"));
    }
    out.retain(|p| p.is_file());
    out
}
