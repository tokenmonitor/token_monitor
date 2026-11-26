//! Real Windows API behavior of the platform crate: the named mutex, the
//! single-instance pipe (client and server), DPAPI failures, owner-only ACL
//! errors, Authenticode on unsigned installers and MSI property reading on
//! packages built with the Windows Installer API itself. Every test is
//! hermetic (temp dirs, per-process pipe names) and cleans up after itself.
#![cfg(windows)]
#![allow(clippy::unwrap_used)]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tokenmon_core::update::{AppVersion, InstallError};
use tokenmon_platform::install::verify::{
    AuthenticodeVerifier, PeHeaderVerifier, SignatureVerifier, zone_id_marks_internet,
};
use tokenmon_platform::install::{PackageVersionReader, SystemVersionReader};
use tokenmon_platform::secrets::{
    DpapiEncryptor, Encryptor, WindowsOwnerOnly, register_owner_only_permissions,
};
use tokenmon_platform::shell::ShellError;
use tokenmon_platform::shell::instance::{InstanceLock, InstanceState};
use tokenmon_platform::shell::ipc::{IpcClient, IpcMessage};
use tokenmon_platform::shell::windows::{
    MutexInstanceLock, PipeClient, PipeServer, current_user_pipe_name,
};

/// A pipe name unique to this process and test.
fn pipe_name(tag: &str) -> String {
    format!(
        "{}.native{}.{tag}",
        current_user_pipe_name().expect("the test user has a SID and a session"),
        std::process::id()
    )
}

/// A minimal structurally valid PE header (no signature, no resources).
fn tiny_pe() -> Vec<u8> {
    let mut pe = vec![0u8; 0x200];
    pe[..2].copy_from_slice(b"MZ");
    pe[0x3C] = 0x80;
    pe[0x80..0x84].copy_from_slice(b"PE\0\0");
    pe
}

/// The named mutex is exclusive: a second acquire is secondary, and releasing
/// the primary guard makes the lock available again.
#[test]
fn test_mutex_lock_is_primary_then_secondary_then_free_again() {
    let lock = MutexInstanceLock;
    let InstanceState::Primary(guard) = lock.try_acquire().expect("first acquire") else {
        panic!("the first acquire must be primary");
    };
    assert!(matches!(
        lock.try_acquire().expect("second acquire"),
        InstanceState::Secondary
    ));
    drop(guard);
    assert!(matches!(
        lock.try_acquire().expect("third acquire"),
        InstanceState::Primary(_)
    ));
}

/// `for_current_user` targets the session pipe name the server derives.
#[test]
fn test_client_for_current_user_uses_the_session_pipe_name() {
    let client = PipeClient::for_current_user().expect("session pipe name");
    let name = current_user_pipe_name().expect("session pipe name");
    assert!(name.starts_with(r"\\.\pipe\TokenMon."));
    assert!(format!("{client:?}").contains(&name.replace('\\', "\\\\")));
}

/// A client with no listener on the pipe exhausts its retries and reports the
/// not-found error as `ShellError::Ipc`.
#[test]
fn test_client_without_a_server_fails_with_a_typed_error_after_retrying() {
    let started = Instant::now();
    let err = PipeClient::new(pipe_name("absent"))
        .send(&IpcMessage::Show)
        .unwrap_err();
    assert!(matches!(err, ShellError::Ipc(_)), "got {err:?}");
    assert!(started.elapsed() >= Duration::from_secs(4), "it retried");
}

/// A name outside the pipe namespace is refused by the server and never
/// hangs the client.
#[test]
fn test_invalid_pipe_names_fail_without_hanging() {
    let started = Instant::now();
    let bad = r"\\.\pipe\\\\bad//name:*?";
    assert!(PipeServer::start(bad, |_| {}).is_err());
    assert!(PipeClient::new(bad).send(&IpcMessage::Show).is_err());
    assert!(started.elapsed() < Duration::from_secs(20));
}

/// A client that connects and closes without writing, then one that writes
/// garbage, are both ignored; the next well-formed client is still served.
#[test]
fn test_server_ignores_silent_and_garbage_clients_then_serves_a_good_one() {
    let name = pipe_name("garbage");
    let (tx, rx) = mpsc::channel();
    let server = PipeServer::start(&name, move |m| {
        let _ = tx.send(m);
    })
    .expect("server starts");

    drop(
        OpenOptions::new()
            .write(true)
            .open(&name)
            .expect("silent client"),
    );
    {
        let mut noisy = OpenOptions::new()
            .write(true)
            .open(&name)
            .expect("garbage client");
        noisy.write_all(b"definitely-not-a-message\n").unwrap();
    }
    PipeClient::new(name.clone())
        .send(&IpcMessage::Show)
        .expect("good client");

    assert_eq!(
        rx.recv_timeout(Duration::from_secs(10)),
        Ok(IpcMessage::Show)
    );
    assert!(rx.try_recv().is_err(), "bad clients produced no message");
    drop(server);
}

/// Several sequential clients are each delivered.
#[test]
fn test_server_delivers_consecutive_messages() {
    let name = pipe_name("seq");
    let (tx, rx) = mpsc::channel();
    let server = PipeServer::start(&name, move |m| {
        let _ = tx.send(m);
    })
    .expect("server starts");
    for _ in 0..3 {
        PipeClient::new(name.clone())
            .send(&IpcMessage::Show)
            .expect("send");
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(10)),
            Ok(IpcMessage::Show)
        );
    }
    drop(server);
}

/// Dropping a server that never had a client returns promptly.
#[test]
fn test_idle_server_drops_promptly() {
    let server = PipeServer::start(&pipe_name("idle"), |_| {}).expect("server starts");
    let started = Instant::now();
    drop(server);
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// DPAPI refuses data it did not produce, with a typed error naming the call,
/// and round-trips real data.
#[test]
fn test_dpapi_rejects_garbage_and_round_trips_real_data() {
    let e = DpapiEncryptor;
    let err = e.unprotect(b"this is not a dpapi blob").unwrap_err();
    assert!(err.0.contains("CryptUnprotectData failed"), "got {err:?}");

    let sealed = e.protect(b"secret").expect("protect");
    assert_ne!(sealed, b"secret");
    assert_eq!(
        e.unprotect(&sealed).expect("unprotect").as_slice(),
        b"secret"
    );
    let mut tampered = sealed;
    let last = tampered.len() - 1;
    tampered[last] ^= 0xff;
    assert!(e.unprotect(&tampered).is_err());
    assert!(e.unprotect(&[]).is_err());
}

/// Restricting a path that does not exist is an error, not a silent success,
/// while real files and directories are restricted.
#[test]
fn test_owner_only_restrict_reports_missing_paths_and_handles_real_ones() {
    use tokenmon_core::storage::OwnerOnlyPermissions;
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    assert!(WindowsOwnerOnly.restrict_file(&missing).is_err());
    assert!(WindowsOwnerOnly.restrict_directory(&missing).is_err());

    let file = dir.path().join("f.txt");
    fs::write(&file, b"x").unwrap();
    WindowsOwnerOnly.restrict_file(&file).expect("file");
    WindowsOwnerOnly
        .restrict_directory(dir.path())
        .expect("dir");
    assert_eq!(fs::read(&file).unwrap(), b"x", "owner still has access");
}

/// Registering the process-wide implementation is accepted at most once.
#[test]
fn test_register_owner_only_permissions_is_once_only() {
    let _first = register_owner_only_permissions();
    assert!(!register_owner_only_permissions(), "second is refused");
}

/// A PE header without a signature is fine for the lenient verifier and
/// refused by Authenticode.
#[test]
fn test_unsigned_pe_passes_header_check_but_fails_authenticode() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("a.exe");
    fs::write(&exe, tiny_pe()).unwrap();

    PeHeaderVerifier
        .verify_exe(&exe)
        .expect("structurally a PE");
    assert_eq!(
        AuthenticodeVerifier.verify_exe(&exe).unwrap_err(),
        InstallError::Verification("The download is not signed by a trusted publisher.".into())
    );
    assert!(
        AuthenticodeVerifier
            .verify_exe(&dir.path().join("missing.exe"))
            .is_err()
    );
}

/// A download marked with the Internet zone stream is refused by the header
/// verifier, and an intranet-zone mark is not.
#[test]
fn test_zone_identifier_stream_blocks_internet_downloads() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("z.exe");
    fs::write(&exe, tiny_pe()).unwrap();

    let stream = format!("{}:Zone.Identifier", exe.display());
    fs::write(&stream, "[ZoneTransfer]\r\nZoneId=2\r\n").expect("NTFS stream");
    PeHeaderVerifier
        .verify_exe(&exe)
        .expect("intranet zone is fine");
    fs::write(&stream, "[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
    assert!(zone_id_marks_internet("[ZoneTransfer]\r\nZoneId=3\r\n"));
    let err = PeHeaderVerifier.verify_exe(&exe).unwrap_err();
    assert!(matches!(err, InstallError::Verification(m) if m.contains("block")));
}

/// MSI verification: unreadable and wrong-magic files are refused, an OLE
/// file passes the structural check and fails Authenticode when unsigned.
#[test]
fn test_msi_verification_checks_magic_then_signature() {
    let dir = tempfile::tempdir().unwrap();
    assert!(
        PeHeaderVerifier
            .verify_msi(&dir.path().join("none.msi"))
            .is_err()
    );
    let junk = dir.path().join("junk.msi");
    fs::write(&junk, b"not an ole file at all").unwrap();
    assert!(PeHeaderVerifier.verify_msi(&junk).is_err());

    let msi = dir.path().join("ok.msi");
    make_msi(&msi, Some("1.2.3"), true);
    PeHeaderVerifier.verify_msi(&msi).expect("OLE container");
    assert!(AuthenticodeVerifier.verify_msi(&msi).is_err());
    assert!(AuthenticodeVerifier.verify_msi(&junk).is_err());
}

/// The reader returns `ProductVersion` from a real package and fails closed
/// for every malformed shape.
#[test]
fn test_msi_version_reads_the_property_table_and_fails_closed() {
    let dir = tempfile::tempdir().unwrap();
    let reader = SystemVersionReader;

    let good = dir.path().join("good.msi");
    make_msi(&good, Some("1.2.3"), true);
    assert_eq!(
        reader.msi_version(&good).expect("version"),
        AppVersion::parse("1.2.3").unwrap()
    );

    let no_row = dir.path().join("norow.msi");
    make_msi(&no_row, None, true);
    assert!(reader.msi_version(&no_row).is_err());

    let no_table = dir.path().join("notable.msi");
    make_msi(&no_table, None, false);
    assert!(reader.msi_version(&no_table).is_err());

    let long = dir.path().join("long.msi");
    make_msi(&long, Some(&"1".repeat(300)), true);
    assert!(reader.msi_version(&long).is_err());

    let text = dir.path().join("text.msi");
    make_msi(&text, Some("not-a-version"), true);
    assert!(reader.msi_version(&text).is_err());

    let junk = dir.path().join("junk.msi");
    fs::write(&junk, b"garbage").unwrap();
    assert!(reader.msi_version(&junk).is_err());
    assert!(reader.msi_version(&dir.path().join("gone.msi")).is_err());
    assert!(reader.exe_version(&dir.path().join("gone.exe")).is_err());
}

/// Builds a Windows Installer database with the Installer API: optionally the
/// `Property` table, optionally a `ProductVersion` row.
fn make_msi(path: &Path, version: Option<&str>, with_table: bool) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::ApplicationInstallationAndServicing::{
        MSIDBOPEN_CREATEDIRECT, MSIHANDLE, MsiCloseHandle, MsiDatabaseCommit, MsiDatabaseOpenViewW,
        MsiOpenDatabaseW, MsiViewClose, MsiViewExecute,
    };

    fn run(db: MSIHANDLE, sql: &str) {
        let q: Vec<u16> = sql.encode_utf16().chain(Some(0)).collect();
        let mut view: MSIHANDLE = 0;
        // SAFETY: `q` is NUL-terminated; `view` is a valid out pointer.
        let opened = unsafe { MsiDatabaseOpenViewW(db, q.as_ptr(), &raw mut view) };
        assert_eq!(opened, 0, "{sql}");
        // SAFETY: `view` is an open view; no parameter record.
        let executed = unsafe { MsiViewExecute(view, 0) };
        assert_eq!(executed, 0, "{sql}");
        // SAFETY: `view` is open and closed once.
        unsafe {
            MsiViewClose(view);
            MsiCloseHandle(view);
        }
    }

    let wpath: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut db: MSIHANDLE = 0;
    // SAFETY: `wpath` is NUL-terminated; `db` is a valid out pointer.
    let created = unsafe { MsiOpenDatabaseW(wpath.as_ptr(), MSIDBOPEN_CREATEDIRECT, &raw mut db) };
    assert_eq!(created, 0);
    if with_table {
        run(
            db,
            "CREATE TABLE `Property` (`Property` CHAR(72) NOT NULL, `Value` LONGCHAR NOT NULL PRIMARY KEY `Property`)",
        );
    }
    if let (true, Some(v)) = (with_table, version) {
        run(
            db,
            &format!(
                "INSERT INTO `Property` (`Property`, `Value`) VALUES ('ProductVersion', '{v}')"
            ),
        );
    }
    // SAFETY: `db` is an open database, committed then closed once.
    let committed = unsafe { MsiDatabaseCommit(db) };
    assert_eq!(committed, 0);
    // SAFETY: closed once.
    unsafe { MsiCloseHandle(db) };
}
