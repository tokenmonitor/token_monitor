//! Checker behaviour under overlapping calls, mid-flight setting changes,
//! MSI installs and the pure helpers (`choose_asset`, `UpdateState`).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use tokenmon_core::BoxFuture;
use tokenmon_core::update::{
    AppVersion, AssetKind, AvailableRelease, InstallError, InstallOutcome, InstallRequest,
    ReleaseAsset, ReleaseFeedFetcher, ReleaseSource, UpdateChecker, UpdateError, UpdateInstaller,
    UpdateSettings, UpdateState, UrlOpener, choose_asset,
};

const DL: &str = "https://github.com/tokenmonitor/token_monitor/releases/download/windows-v1.6.0";

/// A feed that parks in `fetch` until the test releases it, so a second call
/// can be issued while the first check is in flight.
struct GatedFeed {
    body: Result<Vec<u8>, UpdateError>,
    started: tokio::sync::Notify,
    release: tokio::sync::Notify,
    open: AtomicBool,
    requests: AtomicUsize,
}
impl GatedFeed {
    fn new(body: Result<Vec<u8>, UpdateError>) -> Arc<Self> {
        Arc::new(Self {
            body,
            started: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            open: AtomicBool::new(false),
            requests: AtomicUsize::new(0),
        })
    }
}
impl ReleaseFeedFetcher for GatedFeed {
    fn fetch(&self) -> BoxFuture<'_, Result<Vec<u8>, UpdateError>> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            self.started.notify_one();
            if !self.open.load(Ordering::SeqCst) {
                self.release.notified().await;
            }
            self.body.clone()
        })
    }
}

struct Settings(AtomicBool);
impl UpdateSettings for Settings {
    fn checks_for_updates(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct Opener(Mutex<Vec<String>>);
impl UrlOpener for Opener {
    fn open(&self, url: &str) {
        self.0.lock().expect("test invariant").push(url.to_owned());
    }
}

struct Installer {
    can_replace: bool,
    msi: bool,
    result: Result<InstallOutcome, InstallError>,
    requests: Mutex<Vec<InstallRequest>>,
}
impl UpdateInstaller for Installer {
    fn can_replace_running_app(&self) -> bool {
        self.can_replace
    }
    fn installed_by_msi(&self) -> bool {
        self.msi
    }
    fn install(
        &self,
        request: InstallRequest,
    ) -> BoxFuture<'_, Result<InstallOutcome, InstallError>> {
        self.requests.lock().expect("test invariant").push(request);
        let r = self.result.clone();
        Box::pin(async move { r })
    }
}

/// The default `installed_by_msi` is `false`.
struct PlainInstaller;
impl UpdateInstaller for PlainInstaller {
    fn can_replace_running_app(&self) -> bool {
        true
    }
    fn install(
        &self,
        _request: InstallRequest,
    ) -> BoxFuture<'_, Result<InstallOutcome, InstallError>> {
        Box::pin(async { Ok(InstallOutcome::Relaunching) })
    }
}

fn feed_body(assets: &str) -> Vec<u8> {
    format!(
        r#"{{"tag_name":"windows-v1.6.0","html_url":"https://github.com/tokenmonitor/token_monitor/releases/tag/windows-v1.6.0","assets":[{assets}]}}"#
    )
    .into_bytes()
}

fn asset_json(name: &str) -> String {
    format!(
        r#"{{"name":"{name}","browser_download_url":"{DL}/{name}","digest":"sha256:{}"}}"#,
        "d".repeat(64)
    )
}

fn checker(
    fetcher: Arc<dyn ReleaseFeedFetcher>,
    auto: bool,
) -> (Arc<UpdateChecker>, Arc<Opener>, Arc<Settings>) {
    let opener = Arc::new(Opener::default());
    let settings = Arc::new(Settings(AtomicBool::new(auto)));
    let c = Arc::new(UpdateChecker::new(
        ReleaseSource::new("windows-v", "tokenmon-windows-"),
        AppVersion::parse("1.5.0"),
        settings.clone(),
        fetcher,
        opener.clone(),
    ));
    (c, opener, settings)
}

fn asset(name: &str) -> ReleaseAsset {
    ReleaseAsset {
        name: name.to_owned(),
        url: format!("{DL}/{name}"),
        sha256: Some("d".repeat(64)),
    }
}

fn release(zip: bool, msi: bool) -> AvailableRelease {
    AvailableRelease {
        version: AppVersion::parse("1.6.0").expect("test invariant"),
        tag: "windows-v1.6.0".to_owned(),
        page_url: String::new(),
        archive: zip.then(|| asset("a.zip")),
        installer: msi.then(|| asset("a.msi")),
        published_at: None,
    }
}

/// While a check is in flight, a second check (background or manual) issues
/// no second request and leaves the status untouched.
#[tokio::test]
async fn test_overlapping_checks_issue_one_request() {
    let feed = GatedFeed::new(Ok(feed_body("")));
    let (c, ..) = checker(feed.clone(), true);
    let first = {
        let c = c.clone();
        tokio::spawn(async move { c.check_manually().await })
    };
    feed.started.notified().await;
    assert!(c.state().is_checking);
    c.check_now().await;
    c.check_manually().await;
    assert_eq!(feed.requests.load(Ordering::SeqCst), 1);
    assert!(c.state().is_checking && c.state().status_message.is_none());
    feed.release.notify_one();
    first.await.expect("test invariant");
    let s = c.state();
    assert!(!s.is_checking);
    assert_eq!(
        s.available_release
            .expect("test invariant")
            .version
            .to_string(),
        "1.6.0"
    );
}

/// A background result that lands after the user turned automatic checks off
/// is discarded, but the in-flight flag is still cleared.
#[tokio::test]
async fn test_background_result_discarded_when_checks_turned_off_mid_flight() {
    let feed = GatedFeed::new(Ok(feed_body("")));
    let (c, _, settings) = checker(feed.clone(), true);
    let task = {
        let c = c.clone();
        tokio::spawn(async move { c.check_now().await })
    };
    feed.started.notified().await;
    settings.0.store(false, Ordering::SeqCst);
    feed.release.notify_one();
    task.await.expect("test invariant");
    let s = c.state();
    assert!(!s.is_checking);
    assert!(s.available_release.is_none() && s.last_error.is_none());
}

/// A manual check discards nothing even when automatic checks are off.
#[tokio::test]
async fn test_manual_result_kept_when_checks_are_off() {
    let feed = GatedFeed::new(Ok(feed_body("")));
    let (c, ..) = checker(feed.clone(), false);
    let task = {
        let c = c.clone();
        tokio::spawn(async move { c.check_manually().await })
    };
    feed.started.notified().await;
    feed.release.notify_one();
    task.await.expect("test invariant");
    assert!(c.state().available_release.is_some());
}

/// A failed manual check shows the error in the status row; a failed
/// background check records it silently.
#[tokio::test]
async fn test_manual_error_is_shown_but_background_error_is_silent() {
    let feed = GatedFeed::new(Err(UpdateError::from_status(500)));
    feed.open.store(true, Ordering::SeqCst);
    let (c, ..) = checker(feed.clone(), true);
    c.check_now().await;
    let bg = c.state();
    assert_eq!(
        bg.last_error.as_deref(),
        Some("Update check failed: HTTP 500")
    );
    assert!(bg.status_message.is_none());
    c.check_manually().await;
    assert_eq!(
        c.state().status_message.as_deref(),
        Some("Update check failed: HTTP 500")
    );
}

/// With nothing available the primary action performs a manual check and
/// never asks the app to quit.
#[tokio::test]
async fn test_primary_action_without_release_checks_manually() {
    let feed = GatedFeed::new(Ok(feed_body("")));
    feed.open.store(true, Ordering::SeqCst);
    let (c, ..) = checker(feed.clone(), false);
    assert!(!c.perform_primary_action(&PlainInstaller).await);
    assert_eq!(feed.requests.load(Ordering::SeqCst), 1);
    assert!(c.state().available_release.is_some());
}

/// With no release in state an install does nothing; once the slot is released a repeat install is allowed.
#[tokio::test]
async fn test_install_without_release_is_noop_and_repeat_install_allowed() {
    let feed = GatedFeed::new(Ok(feed_body(&asset_json("tokenmon-windows-x64.zip"))));
    feed.open.store(true, Ordering::SeqCst);
    let (c, ..) = checker(feed, true);
    c.check_now().await;
    let inst = Installer {
        can_replace: true,
        msi: false,
        result: Ok(InstallOutcome::Relaunching),
        requests: Mutex::default(),
    };
    assert!(c.install_available_update(&inst).await);
    assert!(c.install_available_update(&inst).await);
    assert_eq!(inst.requests.lock().expect("test invariant").len(), 2);
    // With no release in state there is nothing to install.
    let (empty, ..) = checker(GatedFeed::new(Ok(feed_body(""))), true);
    assert!(!empty.install_available_update(&inst).await);
    assert_eq!(inst.requests.lock().expect("test invariant").len(), 2);
}

/// An MSI install updates through the MSI even when the exe could be swapped.
#[tokio::test]
async fn test_msi_install_prefers_the_msi() {
    let both = format!(
        "{},{}",
        asset_json("tokenmon-windows-x64.zip"),
        asset_json("tokenmon-windows-x64.msi")
    );
    let feed = GatedFeed::new(Ok(feed_body(&both)));
    feed.open.store(true, Ordering::SeqCst);
    let (c, ..) = checker(feed, true);
    c.check_now().await;
    let inst = Installer {
        can_replace: true,
        msi: true,
        result: Ok(InstallOutcome::InstalledNeedsManualRestart),
        requests: Mutex::default(),
    };
    assert!(!c.install_available_update(&inst).await);
    assert_eq!(
        inst.requests.lock().expect("test invariant")[0].asset.name,
        "tokenmon-windows-x64.msi"
    );
}

/// The full `(installed_by_msi, can_replace)` x `(zip, msi)` truth table.
#[test]
fn test_choose_asset_truth_table() {
    let kind = |r: &AvailableRelease, replace, msi| {
        choose_asset(r, replace, msi).map(|(a, k)| (a.name.clone(), k))
    };
    let zip = |n: &str| Some((n.to_owned(), AssetKind::Zip));
    let msi = |n: &str| Some((n.to_owned(), AssetKind::Msi));
    let both = release(true, true);
    assert_eq!(kind(&both, true, true), msi("a.msi"));
    assert_eq!(kind(&both, true, false), zip("a.zip"));
    assert_eq!(kind(&both, false, false), msi("a.msi"));
    let zip_only = release(true, false);
    assert_eq!(kind(&zip_only, true, true), zip("a.zip"));
    assert_eq!(kind(&zip_only, false, true), None);
    assert_eq!(kind(&zip_only, false, false), None);
    let msi_only = release(false, true);
    assert_eq!(kind(&msi_only, true, false), msi("a.msi"));
    assert_eq!(kind(&release(false, false), true, false), None);
}

/// The action title follows the documented priority order.
#[test]
fn test_action_title_priority_and_can_act() {
    let base = UpdateState {
        available_release: Some(release(true, false)),
        ..UpdateState::default()
    };
    assert_eq!(base.action_title(), "Update to 1.6.0…");
    let checking = UpdateState {
        is_checking: true,
        ..base.clone()
    };
    assert_eq!(checking.action_title(), "Checking for updates…");
    assert!(!checking.can_act());
    let installing = UpdateState {
        is_installing: true,
        ..checking
    };
    assert_eq!(installing.action_title(), "Installing update…");
    assert!(!installing.can_act());
    assert!(base.can_act());
}
