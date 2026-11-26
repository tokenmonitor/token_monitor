//! Portable update steps (spec 01 section 3.5): download, archive extraction
//! and the staged swap of the running executable.
//!
//! The logic works on any OS against temp directories (the "running exe" is
//! just a file), so it is tested on every host. Download (80 MiB cap, trusted
//! redirects only), SHA-256 check, extraction with size/symlink/path limits and
//! the swap live here; the per-OS parts (signature and version checks,
//! relaunch, the installer that orchestrates them) live in the OS crates.

pub mod archive;
pub mod download;
pub mod feed;
pub mod swap;

pub use download::{DownloadedFile, Downloader, HttpDownloader, TrustPolicy};
pub use feed::HttpFeedFetcher;

/// File name of the application executable inside the release zip.
pub const EXE_NAME: &str = "TokenMon.exe";
