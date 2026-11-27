//! The spike's vault encryption: a key that lives only in this process.
//!
//! The real beta will keep a Keychain item (M1, `tokenmon-platform-macos`).
//! For M0 nothing secret may reach disk in the clear and no Keychain item may
//! be created, so the vault blob is sealed with a random per-process key: it
//! is unreadable after the process exits, and the credentials are re-entered
//! (environment variable or file) at every start.

use sha2::{Digest, Sha256};
use tokenmon_platform::secrets::{CryptError, Encryptor};
use zeroize::Zeroizing;

const MAGIC: &[u8] = b"TMEPH1:";

/// Seals blobs with a SHA-256 counter-mode keystream keyed per process.
///
/// This is a spike device, not a cipher recommendation: it keeps secrets out
/// of plaintext files for the lifetime of the spike and is replaced by a
/// Keychain-wrapped key in M1.
pub struct EphemeralEncryptor {
    key: [u8; 32],
}

impl EphemeralEncryptor {
    /// An encryptor with the given 32-byte key (use [`Self::random`] in the app).
    pub const fn with_key(key: [u8; 32]) -> Self {
        Self { key }
    }

    /// A fresh random key read from the OS entropy source.
    ///
    /// # Errors
    /// The entropy source could not be read.
    pub fn random() -> std::io::Result<Self> {
        use std::io::Read;
        let mut key = [0_u8; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut key)?;
        Ok(Self { key })
    }

    fn keystream_xor(&self, data: &[u8]) -> Vec<u8> {
        data.chunks(32)
            .enumerate()
            .flat_map(|(i, chunk)| {
                let block = Sha256::new()
                    .chain_update(self.key)
                    .chain_update((i as u64).to_le_bytes())
                    .finalize();
                chunk
                    .iter()
                    .zip(block)
                    .map(|(d, k)| d ^ k)
                    .collect::<Vec<u8>>()
            })
            .collect()
    }
}

impl Encryptor for EphemeralEncryptor {
    fn protect(&self, plain: &[u8]) -> Result<Vec<u8>, CryptError> {
        let mut out = MAGIC.to_vec();
        out.extend(self.keystream_xor(plain));
        Ok(out)
    }

    fn unprotect(&self, blob: &[u8]) -> Result<Zeroizing<Vec<u8>>, CryptError> {
        let body = blob
            .strip_prefix(MAGIC)
            .ok_or_else(|| CryptError("not a blob of this process".into()))?;
        Ok(Zeroizing::new(self.keystream_xor(body)))
    }
}
