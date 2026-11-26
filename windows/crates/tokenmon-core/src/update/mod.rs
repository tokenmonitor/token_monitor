//! Update checking (spec 01 section 3).
//!
//! Everything here is portable and free of I/O: version parsing and ordering
//! ([`AppVersion`]), release-feed parsing and per-channel asset selection
//! ([`feed`]), URL trust rules ([`trust`]), the streaming SHA-256 helper
//! ([`digest`]), the typed errors ([`error`]) and the [`UpdateChecker`] state
//! machine ([`checker`]). The side-effecting half (download, verify, staged
//! replace, relaunch) lives in `tokenmon-platform::install` and plugs in
//! through the [`UpdateInstaller`] trait.
//!
//! Releases of every platform share one repository, so a platform app passes
//! its channel as data ([`ReleaseSource::new`]): a release is only considered
//! when its tag starts with the channel's tag prefix (`windows-v` for the
//! Windows app) and only assets whose names start with its asset prefix
//! (`tokenmon-windows-`) are ever selected, so another platform's file can
//! never be picked.

pub mod checker;
pub mod digest;
pub mod error;
pub mod feed;
pub mod trust;
pub mod version;

pub use checker::{
    InstallOutcome, InstallRequest, UpdateChecker, UpdateInstaller, UpdateSettings, UpdateState,
    UrlOpener, choose_asset,
};
pub use digest::{Sha256Stream, sha256_hex};
pub use error::{InstallError, UpdateError};
pub use feed::{
    AssetKind, AvailableRelease, MAX_FEED_BYTES, ReleaseAsset, ReleaseFeedFetcher, ReleaseSource,
    newer_release, sha256_from_digest,
};
pub use trust::{is_repository_release_download, is_repository_release_page, is_trusted_download};
pub use version::AppVersion;

/// How often the background check runs, in seconds (6 hours).
pub const CHECK_INTERVAL_SECS: u64 = 6 * 3600;

/// Hard cap on a downloaded release file: 80 MiB.
pub const MAX_DOWNLOAD_BYTES: u64 = 80 * 1024 * 1024;

/// Hard cap on the total size of an extracted archive: 256 MiB.
pub const MAX_EXTRACTED_BYTES: u64 = 256 * 1024 * 1024;
