//! The real, read-only Keychain source (macOS Security framework).
//!
//! Only `SecItemCopyMatching` (through `get_generic_password`) is ever called:
//! there is no add, update or delete anywhere in this crate. macOS shows its
//! normal "Always Allow" prompt the first time the beta reads the Swift app's
//! item.

use security_framework::base::Error;
use security_framework::passwords::get_generic_password;
use zeroize::Zeroizing;

use crate::keychain::{SecretSource, SourceError};

/// `errSecItemNotFound`.
const ITEM_NOT_FOUND: i32 = -25300;
/// `errSecAuthFailed`, `errSecInteractionNotAllowed`, `errSecUserCanceled`.
const DENIED: [i32; 3] = [-25293, -25308, -128];

/// Reads generic passwords from the login Keychain.
pub struct MacKeychain;

impl SecretSource for MacKeychain {
    fn read(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<Zeroizing<Vec<u8>>>, SourceError> {
        match get_generic_password(service, account) {
            Ok(data) => Ok(Some(Zeroizing::new(data))),
            Err(e) => classify(e),
        }
    }
}

fn classify(e: Error) -> Result<Option<Zeroizing<Vec<u8>>>, SourceError> {
    match e.code() {
        ITEM_NOT_FOUND => Ok(None),
        code if DENIED.contains(&code) => Err(SourceError::Denied),
        code => Err(SourceError::Failed(code)),
    }
}
