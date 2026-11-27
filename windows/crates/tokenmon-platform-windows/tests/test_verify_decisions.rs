//! The structural verification decisions, exercised on every host: the
//! Internet-zone refusal, the MSI container check and the Authenticode
//! verifier's refusal of unsigned packages.

use std::io::Write;
use std::path::Path;

use tokenmon_core::update::InstallError;
use tokenmon_platform_windows::install::verify::zone_stream_path;
use tokenmon_platform_windows::install::{
    AuthenticodeVerifier, PeHeaderVerifier, SignatureVerifier,
};

const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

fn temp_with(bytes: &[u8]) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().expect("temp file");
    f.write_all(bytes).expect("write");
    f
}

fn pe_file() -> tempfile::NamedTempFile {
    let mut bytes = vec![0u8; 128];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3C] = 0x40;
    bytes[0x40..0x44].copy_from_slice(b"PE\0\0");
    temp_with(&bytes)
}

fn msi_file() -> tempfile::NamedTempFile {
    let mut bytes = OLE_MAGIC.to_vec();
    bytes.extend_from_slice(b"msi body");
    temp_with(&bytes)
}

/// A PE marked as downloaded from the Internet (ZoneId 3) or a restricted
/// zone (4) is refused with a verification error; no stream, a local zone or
/// unparsable text is accepted.
#[test]
fn test_internet_zone_marked_pe_is_refused() {
    let pe = pe_file();
    for zone in ["[ZoneTransfer]\r\nZoneId=3\r\n", "ZoneId=4"] {
        let err = PeHeaderVerifier
            .verify_exe_with_zone(pe.path(), Some(zone))
            .expect_err("blocked");
        assert!(
            matches!(&err, InstallError::Verification(m) if m.contains("block")),
            "{err:?}"
        );
    }
    for zone in [None, Some("[ZoneTransfer]\r\nZoneId=2"), Some("garbage")] {
        assert!(
            PeHeaderVerifier
                .verify_exe_with_zone(pe.path(), zone)
                .is_ok()
        );
    }
}

/// A file that is not a PE image is refused before the zone is consulted, and
/// so is a path that does not exist.
#[test]
fn test_non_pe_is_refused_regardless_of_zone() {
    let junk = temp_with(b"hello");
    for zone in [None, Some("ZoneId=0")] {
        assert!(matches!(
            PeHeaderVerifier.verify_exe_with_zone(junk.path(), zone),
            Err(InstallError::Verification(m)) if m.contains("not a Windows program")
        ));
    }
    assert!(
        PeHeaderVerifier
            .verify_exe_with_zone(Path::new("no-such-file.exe"), None)
            .is_err()
    );
}

/// The real `verify_exe` accepts a structurally valid PE that has no zone
/// stream (every temp file here).
#[test]
fn test_verify_exe_accepts_a_plain_pe() {
    assert!(PeHeaderVerifier.verify_exe(pe_file().path()).is_ok());
}

/// An MSI is the OLE compound-file container: the magic passes, other bytes
/// and unreadable paths are refused with distinct messages.
#[test]
fn test_msi_container_check() {
    assert!(PeHeaderVerifier.verify_msi(msi_file().path()).is_ok());
    assert!(matches!(
        PeHeaderVerifier.verify_msi(temp_with(b"MZ not an msi").path()),
        Err(InstallError::Verification(m)) if m.contains("not a Windows installer")
    ));
    assert!(matches!(
        PeHeaderVerifier.verify_msi(Path::new("no-such.msi")),
        Err(InstallError::Verification(m)) if m.contains("could not be read")
    ));
}

/// The Authenticode verifier refuses an unsigned MSI after the structural
/// check passes, and refuses a non-MSI at the structural check.
#[test]
fn test_authenticode_verifier_refuses_unsigned_packages() {
    let msi = msi_file();
    assert!(matches!(
        AuthenticodeVerifier.verify_msi(msi.path()),
        Err(InstallError::Verification(_))
    ));
    assert!(matches!(
        AuthenticodeVerifier.verify_msi(temp_with(b"nope").path()),
        Err(InstallError::Verification(m)) if m.contains("not a Windows installer")
    ));
}

/// The zone stream of a file is its path plus `:Zone.Identifier`.
#[test]
fn test_zone_stream_path_appends_the_stream_name() {
    assert_eq!(
        zone_stream_path(Path::new(r"C:\dl\TokenMon.exe")).to_string_lossy(),
        r"C:\dl\TokenMon.exe:Zone.Identifier"
    );
}
