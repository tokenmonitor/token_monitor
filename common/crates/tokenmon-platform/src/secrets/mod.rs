//! The encrypted vault file (`vault.bin` in the data directory) and the
//! credential stores built on it (spec 01 sections 1.6.1 to 1.6.3).
//!
//! Layers, all portable; the OS encryptor lives in the OS crates:
//!
//! * [`Encryptor`]: the OS encryption seam (DPAPI on Windows, [`FakeEncryptor`]
//!   in tests).
//! * [`decode_payload`] / [`encode_payload`]: the decrypted vault JSON format.
//!   The decoder takes untrusted bytes and is fuzzed.
//! * [`Vault`]: one encrypted file holding every provider's secrets, with
//!   write-then-read-back verification, pending removals and the
//!   "unavailable means locked out for the process" rule.
//! * [`FileBackedCredentialStore`]: plain owner-only `<prefix><key>.dat` files
//!   for non-secret labels only (email, identity, workspace id).
//! * [`SecretRoutingCredentialStore`]: the production
//!   [`CredentialStore`](tokenmon_core::CredentialStore) that routes by
//!   [`CredentialKind`](tokenmon_core::CredentialKind).
//!
//! Secrets are held in [`zeroize::Zeroizing`] buffers and are never logged.

mod encryptor;
mod files;
mod payload;
mod routing;
mod secure_file;
mod vault;

pub use encryptor::{CryptError, Encryptor, FakeEncryptor};
pub use files::{FileBackedCredentialStore, atomic_write, secure_delete};
pub use payload::{
    MAX_PAYLOAD_BYTES, PayloadError, VaultPayload, decode_payload, decode_pending, encode_payload,
};
pub use routing::{SecretRoutingCredentialStore, SecretStoreError, VAULT_UNAVAILABLE_REASON};
pub use secure_file::{
    OwnerOnlyFileCreator, create_new_default, create_new_owner_only,
    register_owner_only_file_creator,
};
pub use vault::{PENDING_FILE_NAME, VAULT_FILE_NAME, Vault};
