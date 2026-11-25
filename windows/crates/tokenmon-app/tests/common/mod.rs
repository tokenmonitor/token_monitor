//! Shared fixtures for the typed-source tests: signed-in sessions over a fake
//! vault, a fixed clock and a ready `ViewQuery`.
#![allow(dead_code)]

use std::sync::Arc;

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use tokenmon_app::sessions::{HostBrowserCleaner, Sessions};
use tokenmon_app::sources::ViewQuery;
use tokenmon_core::settings::{AppSettings, Settings, SettingsFile};
use tokenmon_core::{BrowserStateCleaner, Clock, ManualClock, ProviderAuthSession, ProviderId};
use tokenmon_platform::secrets::{FakeEncryptor, Vault};
use tokenmon_platform::webauth::{FakeWebAuthHost, WebAuthHost};

/// The fixed "now" of every source test (a Wednesday, 15:00 UTC).
pub fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, 15, 0, 0)
        .single()
        .expect("valid instant")
}

/// A manual clock at [`t0`], six hours west of UTC.
pub fn clock() -> Arc<ManualClock> {
    Arc::new(
        ManualClock::new(t0()).with_offset(FixedOffset::west_opt(6 * 3600).expect("valid offset")),
    )
}

/// Real sessions over a fake vault in a temp directory.
pub struct Env {
    pub _dir: tempfile::TempDir,
    pub sessions: Sessions,
    pub clock: Arc<ManualClock>,
    pub settings: Arc<AppSettings>,
}

/// Builds an [`Env`] with every session signed out.
pub fn env() -> Env {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = Arc::new(Vault::new(dir.path(), Arc::new(FakeEncryptor::default())));
    let clock = clock();
    let dyn_clock: Arc<dyn Clock> = clock.clone();
    let host: Arc<dyn WebAuthHost> = Arc::new(FakeWebAuthHost::default());
    let cleaner: Arc<dyn BrowserStateCleaner> = Arc::new(HostBrowserCleaner::new(host));
    let sessions = Sessions::open(dir.path(), &vault, &dyn_clock, &cleaner);
    Env {
        _dir: dir,
        sessions,
        clock,
        settings: Arc::new(AppSettings::new(Arc::new(SettingsFile::in_memory()))),
    }
}

impl Env {
    /// Signs `provider`'s cookie session in with a throwaway cookie.
    pub fn sign_in(&self, provider: ProviderId) {
        let session: &Arc<tokenmon_core::CookieAuthSession> = match provider {
            ProviderId::Grok => &self.sessions.grok,
            ProviderId::Cursor | ProviderId::Grokbot => &self.sessions.cursor,
            ProviderId::Claude => &self.sessions.claude,
            ProviderId::ChatGpt => &self.sessions.chatgpt,
            ProviderId::OpenCode => &self.sessions.opencode,
            other => panic!("{other:?} has no cookie session"),
        };
        session.save("test=cookie").expect("cookie stored");
    }

    /// The clock as the trait object the sources and stores take.
    pub fn dyn_clock(&self) -> Arc<dyn Clock> {
        self.clock.clone()
    }

    /// The session of a cookie provider as the poller-facing trait object.
    pub fn auth(&self, provider: ProviderId) -> Arc<dyn ProviderAuthSession> {
        match provider {
            ProviderId::Grok => self.sessions.grok.clone(),
            ProviderId::Cursor | ProviderId::Grokbot => self.sessions.cursor.clone(),
            ProviderId::Claude => self.sessions.claude.clone(),
            ProviderId::ChatGpt => self.sessions.chatgpt.clone(),
            ProviderId::OpenCode => self.sessions.opencode.clone(),
            other => panic!("{other:?} has no cookie session"),
        }
    }
}

/// A query at [`t0`] for `week_offset` with the given settings.
pub fn query(clock: &dyn Clock, week_offset: i32, settings: Settings) -> ViewQuery<'_> {
    ViewQuery {
        now: t0(),
        clock,
        week_offset,
        show_all_models: false,
        settings,
    }
}
