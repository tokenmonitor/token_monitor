//! IPC reader error paths: failing readers, interruptions, zero deadlines and
//! the in-memory fake.

#![allow(clippy::assert_is_empty)]

use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use tokenmon_platform::shell::ipc::{DeadlineRead, FakeIpc, IpcClient};
use tokenmon_platform::shell::{IpcMessage, IpcParseError};

/// One scripted outcome of a `read_some` call.
enum Step {
    Data(&'static [u8]),
    Fail(io::ErrorKind),
}

struct Script(VecDeque<Step>);

impl DeadlineRead for Script {
    fn read_some(&mut self, buf: &mut [u8], _timeout: Duration) -> io::Result<usize> {
        match self.0.pop_front() {
            None => Ok(0),
            Some(Step::Fail(kind)) => Err(kind.into()),
            Some(Step::Data(bytes)) => {
                buf[..bytes.len()].copy_from_slice(bytes);
                Ok(bytes.len())
            }
        }
    }
}

fn script(steps: Vec<Step>) -> Script {
    Script(steps.into())
}

struct BrokenReader;
impl io::Read for BrokenReader {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
}

/// A read failure on the blocking reader is reported as `Unknown`, while a
/// well-formed stream decodes.
#[test]
fn test_read_from_maps_io_errors_and_decodes_streams() {
    assert_eq!(
        IpcMessage::read_from(BrokenReader),
        Err(IpcParseError::Unknown)
    );
    assert_eq!(
        IpcMessage::read_from(&b"TM1 show\r\n"[..]),
        Ok(IpcMessage::Show)
    );
}

/// A zero deadline times out before any read happens.
#[test]
fn test_zero_deadline_times_out_without_reading() {
    let mut s = script(vec![Step::Data(b"TM1 show\n")]);
    assert_eq!(
        IpcMessage::read_with_deadline(&mut s, Duration::ZERO),
        Err(IpcParseError::TimedOut)
    );
    assert_eq!(s.0.len(), 1, "no read may be issued");
}

/// An interrupted read is retried transparently.
#[test]
fn test_interrupted_read_is_retried() {
    let mut s = script(vec![
        Step::Fail(io::ErrorKind::Interrupted),
        Step::Data(b"TM1 show\n"),
    ]);
    assert_eq!(
        IpcMessage::read_with_deadline(&mut s, Duration::from_secs(1)),
        Ok(IpcMessage::Show)
    );
}

/// Any other read error aborts with `Unknown`; a timeout error maps to `TimedOut`.
#[test]
fn test_other_read_errors_abort_and_timeouts_map() {
    let mut s = script(vec![Step::Fail(io::ErrorKind::ConnectionReset)]);
    assert_eq!(
        IpcMessage::read_with_deadline(&mut s, Duration::from_secs(1)),
        Err(IpcParseError::Unknown)
    );
    let mut s = script(vec![
        Step::Data(b"TM1 "),
        Step::Fail(io::ErrorKind::TimedOut),
    ]);
    assert_eq!(
        IpcMessage::read_with_deadline(&mut s, Duration::from_secs(1)),
        Err(IpcParseError::TimedOut)
    );
}

/// An empty stream is an unknown (empty) message, not a hang or a success.
#[test]
fn test_empty_stream_is_unknown() {
    let mut s = script(vec![]);
    assert_eq!(
        IpcMessage::read_with_deadline(&mut s, Duration::from_secs(1)),
        Err(IpcParseError::Unknown)
    );
}

/// The default fake is reachable and records what was sent; the unreachable
/// fake refuses.
#[test]
fn test_fake_ipc_default_records_and_unreachable_refuses() {
    let fake = FakeIpc::default();
    assert!(fake.received().is_empty());
    fake.send(&IpcMessage::Show).expect("reachable");
    assert_eq!(fake.received(), vec![IpcMessage::Show]);
    assert!(FakeIpc::unreachable().send(&IpcMessage::Show).is_err());
}
