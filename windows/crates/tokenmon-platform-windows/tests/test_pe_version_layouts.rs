//! PE header and resource-tree layouts the version reader must accept (PE32+)
//! or refuse (bad magic, short or empty directories, broken trees), plus the
//! production reader's fail-closed behaviour on unreadable files.

mod common;

use std::io::Cursor;

use common::pe_with_version;
use tokenmon_core::update::InstallError;
use tokenmon_platform_windows::install::pe_version::{
    PackageVersionReader, SystemVersionReader, pe_has_icon_group, read_pe_version,
};

/// Offset of the PE signature in the helper's images.
const PE: usize = 0x40;
/// Offset of the optional header (after the 20-byte COFF header).
const OPT: usize = PE + 24;
/// Offset of the resource blob inside the image (`.rsrc` raw data + 88).
const BLOB: usize = 0x200 + 88;

fn read(bytes: Vec<u8>) -> Option<String> {
    read_pe_version(Cursor::new(bytes)).map(|v| v.to_string())
}

fn good() -> Vec<u8> {
    pe_with_version(Some("1.6.0"), Some([1, 6, 0, 0]), "")
}

fn put_u16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn put_u32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// Rewrites the helper's PE32 image as PE32+ (a 112-byte data-directory
/// offset and a 240-byte optional header), keeping the same resource tree.
fn as_pe32_plus(pe32: &[u8]) -> Vec<u8> {
    let mut out = pe32[..OPT].to_vec();
    let mut opt = vec![0u8; 240];
    opt[..2].copy_from_slice(&0x20Bu16.to_le_bytes());
    opt[108..112].copy_from_slice(&16u32.to_le_bytes());
    opt[112 + 16..112 + 24].copy_from_slice(&pe32[OPT + 96 + 16..OPT + 96 + 24]);
    out.extend_from_slice(&opt);
    out.extend_from_slice(&pe32[OPT + 224..OPT + 224 + 40]);
    put_u16(&mut out, PE + 20, 240);
    out.resize(0x200, 0);
    out.extend_from_slice(&pe32[0x200..]);
    out
}

/// A PE32+ image with the same resources reads the same version.
#[test]
fn test_pe32_plus_images_are_supported() {
    let pe = as_pe32_plus(&good());
    assert_eq!(read(pe), Some("1.6.0".to_owned()));
}

/// An unknown optional-header magic is not a PE we understand.
#[test]
fn test_unknown_optional_header_magic_is_refused() {
    let mut pe = good();
    put_u16(&mut pe, OPT, 0x1234);
    assert_eq!(read(pe), None);
}

/// An optional header too small to hold the data directories is refused.
#[test]
fn test_short_optional_header_is_refused() {
    let mut pe = good();
    put_u16(&mut pe, PE + 20, 100);
    assert_eq!(read(pe), None);
}

/// Fewer than three data directories means there is no resource directory.
#[test]
fn test_too_few_data_directories_is_refused() {
    let mut pe = good();
    put_u32(&mut pe, OPT + 92, 2);
    assert_eq!(read(pe), None);
}

/// A zero resource RVA or size means the image has no resources.
#[test]
fn test_empty_resource_directory_is_refused() {
    let mut no_rva = good();
    put_u32(&mut no_rva, OPT + 96 + 16, 0);
    assert_eq!(read(no_rva), None);
    let mut no_size = good();
    put_u32(&mut no_size, OPT + 96 + 20, 0);
    assert_eq!(read(no_size), None);
}

/// A resource RVA that no section covers cannot be located on disk.
#[test]
fn test_resource_rva_outside_every_section_is_refused() {
    let mut pe = good();
    put_u32(&mut pe, OPT + 96 + 16, 0x9000);
    assert_eq!(read(pe), None);
}

/// A missing PE signature or a DOS header pointing nowhere is refused.
#[test]
fn test_bad_signatures_are_refused() {
    let mut no_pe = good();
    no_pe[PE] = b'X';
    assert_eq!(read(no_pe), None);
    let mut no_mz = good();
    no_mz[0] = b'X';
    assert_eq!(read(no_mz), None);
    let mut far = good();
    put_u32(&mut far, 0x3C, u32::MAX);
    assert_eq!(read(far), None);
}

/// A resource tree whose type entry is not a directory, or whose language
/// entry is one, is not a valid version tree.
#[test]
fn test_malformed_resource_trees_are_refused() {
    let mut leaf_at_root = good();
    // First directory's only entry target: clear the "is a directory" bit.
    put_u32(&mut leaf_at_root, 0x200 + 20, 0x18);
    assert_eq!(read(leaf_at_root), None);
    let mut dir_at_leaf = good();
    // Third directory's entry target: set the directory bit on the data entry.
    put_u32(&mut dir_at_leaf, 0x200 + 0x30 + 20, 0x8000_0048);
    assert_eq!(read(dir_at_leaf), None);
    // A type entry that is not RT_VERSION (and not a group icon either).
    let mut other_type = good();
    put_u32(&mut other_type, 0x200 + 16, 99);
    assert_eq!(read(other_type.clone()), None);
    assert!(!pe_has_icon_group(Cursor::new(other_type)));
}

/// A version key that never terminates within the node is refused, as is a
/// fixed-info block with the wrong signature when no product string exists.
#[test]
fn test_hostile_version_nodes_are_refused() {
    let mut long_key = good();
    let end = long_key.len();
    for i in (BLOB + 6..end).step_by(2) {
        put_u16(&mut long_key, i, 0x41);
    }
    assert_eq!(read(long_key), None);
    let mut bad_sig = pe_with_version(None, Some([1, 6, 0, 0]), "");
    put_u32(&mut bad_sig, BLOB + 40, 0);
    assert_eq!(read(bad_sig), None);
}

/// The production reader fails closed with a verification error for files it
/// cannot read, for non-PE content and for non-MSI content.
#[test]
fn test_system_reader_fails_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let reader = SystemVersionReader;
    let missing = dir.path().join("missing.exe");
    assert!(matches!(
        reader.exe_version(&missing),
        Err(InstallError::Verification(_))
    ));
    let junk = dir.path().join("junk.bin");
    std::fs::write(&junk, b"not an executable").expect("write");
    assert!(matches!(
        reader.exe_version(&junk),
        Err(InstallError::Verification(_))
    ));
    assert!(matches!(
        reader.msi_version(&junk),
        Err(InstallError::Verification(_))
    ));
    let pe = dir.path().join("ok.exe");
    std::fs::write(&pe, good()).expect("write");
    assert_eq!(reader.exe_version(&pe).expect("reads").to_string(), "1.6.0");
}
