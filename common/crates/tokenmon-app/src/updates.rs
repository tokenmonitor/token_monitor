//! Updates: the periodic release check and the in-place installer (spec 01
//! section 3). The platform app supplies the release channel
//! ([`ReleaseSource`]) and the installer that knows how to replace its own
//! executable; this module wires them to the checker and the periodic loop.

use std::sync::Arc;
use std::time::Duration;

use tokenmon_core::settings::AppSettings;
use tokenmon_core::update::feed::ReleaseFeedFetcher;
use tokenmon_core::update::{
    AppVersion, CHECK_INTERVAL_SECS, ReleaseSource, UpdateChecker, UpdateInstaller, UpdateSettings,
    UrlOpener,
};
use tokenmon_platform::install::HttpFeedFetcher;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

/// Reads the "check for updates" switch from the live settings.
struct SettingsSwitch(Arc<AppSettings>);

impl UpdateSettings for SettingsSwitch {
    fn checks_for_updates(&self) -> bool {
        self.0.snapshot().checks_for_updates
    }
}

/// The update checker and installer.
pub struct UpdateService {
    /// Checks the release feed and holds the update state the panel shows.
    pub checker: Arc<UpdateChecker>,
    /// Installs an available update over the running executable.
    pub installer: Arc<dyn UpdateInstaller>,
}

impl UpdateService {
    /// Wires the checker over the real GitHub release feed for `source`, with
    /// `installer` doing the platform-specific replace.
    ///
    /// # Errors
    /// The HTTP stack could not start.
    pub fn with_http_feed(
        source: ReleaseSource,
        settings: &Arc<AppSettings>,
        version: &str,
        opener: Arc<dyn UrlOpener>,
        installer: Arc<dyn UpdateInstaller>,
    ) -> Result<Self, String> {
        let user_agent = tokenmon_core::util::user_agent(Some(version));
        let fetcher: Arc<dyn ReleaseFeedFetcher> = Arc::new(
            HttpFeedFetcher::new(source.releases_url(), &user_agent).map_err(|e| e.to_string())?,
        );
        Ok(Self {
            checker: Arc::new(UpdateChecker::new(
                source,
                AppVersion::parse(version),
                Arc::new(SettingsSwitch(Arc::clone(settings))),
                fetcher,
                opener,
            )),
            installer,
        })
    }

    /// Checks now and then every [`CHECK_INTERVAL_SECS`] until aborted. A
    /// disabled switch makes each check a no-op.
    pub fn spawn_loop(&self, handle: &Handle) -> JoinHandle<()> {
        let checker = Arc::clone(&self.checker);
        handle.spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(CHECK_INTERVAL_SECS));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                checker.check_now().await;
            }
        })
    }
}
