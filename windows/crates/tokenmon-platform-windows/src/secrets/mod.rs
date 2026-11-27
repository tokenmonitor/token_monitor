//! The Windows secret store: DPAPI encryption and SID-based owner-only files.
//!
//! The vault, the payload codec and the credential stores are portable and live
//! in `tokenmon_platform::secrets`.

#[cfg(windows)]
mod dpapi;
mod secure_file;

#[cfg(windows)]
pub use dpapi::{DpapiEncryptor, blob_len};
#[cfg(windows)]
pub(crate) use secure_file::current_user_sid;
pub use secure_file::{
    WindowsOwnerOnly, create_new_owner_only, owner_only_directory_sddl, owner_only_sddl,
    register_owner_only_permissions,
};
