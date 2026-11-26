//! Zip extraction edge cases: entry-count cap, directory entries, destination
//! conflicts, lying size headers and `locate_exe` file-type checks.

use std::fs;
use std::io::Write;

use tokenmon_core::update::InstallError;
use tokenmon_platform::install::archive::{extract_zip, locate_exe};
use zip::write::SimpleFileOptions;

fn archive_msg(e: InstallError) -> String {
    match e {
        InstallError::Archive(m) => m,
        other => panic!("expected archive error, got {other:?}"),
    }
}

fn zip_of(build: impl FnOnce(&mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>)) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    build(&mut w);
    w.finish().expect("zip finishes").into_inner()
}

/// More than 10 000 entries is refused before anything is extracted.
#[test]
fn test_too_many_entries_is_refused() {
    let bytes = zip_of(|w| {
        for i in 0..10_001 {
            w.start_file(format!("f{i}"), SimpleFileOptions::default())
                .expect("start file");
        }
    });
    let dir = tempfile::tempdir().expect("tempdir");
    let zip = dir.path().join("a.zip");
    fs::write(&zip, bytes).expect("write zip");
    let out = dir.path().join("out");
    let e = extract_zip(&zip, &out, 1 << 20).expect_err("too many entries");
    assert_eq!(archive_msg(e), "The release zip has too many files.");
    assert!(!out.exists());
}

/// Directory entries (including empty ones) are created on disk.
#[test]
fn test_directory_entries_are_created() {
    let bytes = zip_of(|w| {
        w.add_directory("empty/", SimpleFileOptions::default())
            .expect("dir");
        w.start_file("deep/er/file.txt", SimpleFileOptions::default())
            .expect("file");
        w.write_all(b"hi").expect("write");
    });
    let dir = tempfile::tempdir().expect("tempdir");
    let zip = dir.path().join("a.zip");
    fs::write(&zip, bytes).expect("write zip");
    let out = dir.path().join("out");
    extract_zip(&zip, &out, 1 << 20).expect("extracts");
    assert!(out.join("empty").is_dir());
    assert_eq!(fs::read(out.join("deep/er/file.txt")).expect("read"), b"hi");
}

/// A destination that is a regular file cannot be created and is reported.
#[test]
fn test_destination_that_is_a_file_is_reported() {
    let bytes = zip_of(|w| {
        w.start_file("a.txt", SimpleFileOptions::default())
            .expect("file");
    });
    let dir = tempfile::tempdir().expect("tempdir");
    let zip = dir.path().join("a.zip");
    fs::write(&zip, bytes).expect("write zip");
    let dest = dir.path().join("occupied");
    fs::write(&dest, b"x").expect("write dest");
    let e = extract_zip(&zip, &dest, 1 << 20).expect_err("dest is a file");
    assert_eq!(archive_msg(e), "The release zip could not be read.");
}

/// A file entry whose parent path is already a file, and a directory entry
/// colliding with a file, both fail cleanly rather than overwriting.
#[test]
fn test_entry_colliding_with_existing_file_is_reported() {
    let bytes = zip_of(|w| {
        w.start_file("blocker", SimpleFileOptions::default())
            .expect("file");
        w.start_file("blocker/child.txt", SimpleFileOptions::default())
            .expect("file");
    });
    let dir = tempfile::tempdir().expect("tempdir");
    let zip = dir.path().join("a.zip");
    fs::write(&zip, bytes).expect("write zip");
    let e = extract_zip(&zip, &dir.path().join("out"), 1 << 20).expect_err("collision");
    assert_eq!(archive_msg(e), "The release zip could not be read.");

    let bytes = zip_of(|w| {
        w.start_file("thing", SimpleFileOptions::default())
            .expect("file");
        w.add_directory("thing/", SimpleFileOptions::default())
            .expect("dir");
    });
    fs::write(&zip, bytes).expect("write zip");
    let e = extract_zip(&zip, &dir.path().join("out2"), 1 << 20).expect_err("dir over file");
    assert_eq!(archive_msg(e), "The release zip could not be read.");
}

/// A header that understates the uncompressed size must not let more than
/// `max_total` bytes onto disk: extraction fails or stays within the cap.
#[test]
fn test_lying_size_header_cannot_exceed_the_cap() {
    let payload = vec![0u8; 100_000];
    let mut bytes = zip_of(|w| {
        let opts =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        w.start_file("bomb.bin", opts).expect("file");
        w.write_all(&payload).expect("write");
    });
    // Patch the uncompressed size to 10 in the local and central headers.
    for (sig, off) in [
        ([0x50, 0x4b, 0x03, 0x04], 22),
        ([0x50, 0x4b, 0x01, 0x02], 24),
    ] {
        let pos = bytes
            .windows(4)
            .position(|w| w == sig)
            .expect("header present");
        bytes[pos + off..pos + off + 4].copy_from_slice(&10u32.to_le_bytes());
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let zip = dir.path().join("a.zip");
    fs::write(&zip, bytes).expect("write zip");
    let out = dir.path().join("out");
    let result = extract_zip(&zip, &out, 1000);
    let on_disk = fs::metadata(out.join("bomb.bin")).map_or(0, |m| m.len());
    assert!(on_disk <= 1001, "wrote {on_disk} bytes past the cap");
    if let Err(e) = result {
        assert!(archive_msg(e).starts_with("The release zip"));
    }
}

/// `TokenMon.exe` being a directory is not an exe.
#[test]
fn test_locate_exe_rejects_a_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::create_dir(dir.path().join("TokenMon.exe")).expect("mkdir");
    let e = locate_exe(dir.path()).expect_err("directory is not an exe");
    assert!(archive_msg(e).contains("top level"));
}

/// `TokenMon.exe` being a symlink is refused even when it points at a file.
#[cfg(unix)]
#[test]
fn test_locate_exe_rejects_a_symlink() {
    let dir = tempfile::tempdir().expect("tempdir");
    let real = dir.path().join("real.exe");
    fs::write(&real, b"x").expect("write");
    std::os::unix::fs::symlink(&real, dir.path().join("TokenMon.exe")).expect("symlink");
    let e = locate_exe(dir.path()).expect_err("symlink refused");
    assert_eq!(archive_msg(e), "The release zip contains a symbolic link.");
}
