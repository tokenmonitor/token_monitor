//! Owner-only file creation.
//!
//! The secrets files (`vault.bin`, `vault_pending_removals.json`, the plain
//! `.dat` identity values) must never be readable by another account, not even
//! for the instant between create and a later "restrict" call.
//!
//! * Unix: the file is created with mode 0600 (`open(2)` applies it atomically).
//! * Windows: the Windows platform crate registers a creator
//!   ([`register_owner_only_file_creator`]) that applies a protected DACL
//!   granting only the current user inside `CreateFileW`, so there is no window
//!   where the file carries the inherited ACL. Without one, a file is created
//!   plainly and inherits the (already owner-only) data directory's ACL.
//!
//! A rename keeps the DACL, so the staging file's ACL becomes the target's.

use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::OnceLock;

/// Creates a new file that only the current user can read and write.
pub type OwnerOnlyFileCreator = fn(&Path) -> io::Result<File>;

static CREATOR: OnceLock<OwnerOnlyFileCreator> = OnceLock::new();

/// Installs the platform's owner-only file creator for the process.
///
/// Returns `false` (and changes nothing) when one is already installed. The
/// Windows app registers its atomic-DACL creator once at startup, before any
/// secret is written.
pub fn register_owner_only_file_creator(creator: OwnerOnlyFileCreator) -> bool {
    CREATOR.set(creator).is_ok()
}

/// Creates `path` (it must not exist) readable and writable by the current
/// user only, and returns it opened for writing.
///
/// Uses the registered creator ([`register_owner_only_file_creator`]), else
/// [`create_new_default`].
///
/// # Errors
///
/// The file exists already, or cannot be created.
pub fn create_new_owner_only(path: &Path) -> io::Result<File> {
    match CREATOR.get() {
        Some(create) => create(path),
        None => create_new_default(path),
    }
}

/// The portable creator: `create_new` with mode 0600 on Unix, a plain
/// `create_new` elsewhere (the registered platform creator replaces it there).
///
/// # Errors
///
/// The file exists already, or cannot be created.
pub fn create_new_default(path: &Path) -> io::Result<File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    opts.open(path)
}
