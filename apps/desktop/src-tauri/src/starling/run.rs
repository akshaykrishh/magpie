// Running other tools without ever waiting on them for long. Everything
// that shells out for the starling integration (starling itself, `ps`, the
// login shell, git) goes through `run`, so a hung binary costs a bounded
// wait on a background thread and never a stuck window.

use std::io::{ErrorKind, Read};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Every call the integration makes gives up after this long -- the same
/// 2 seconds docs/integration.md promises for the contract's own calls.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(2);

/// More output than any real answer: a runaway command can't make this
/// process buffer without bound.
const MAX_OUTPUT: u64 = 4 * 1024 * 1024;

pub struct Captured {
    pub success: bool,
    pub stdout: String,
}

/// `spawn`, retried briefly on "text file busy": another thread forking
/// while a script was being written (an installer, or a test creating a
/// fake binary) leaves a window where exec of that file fails, and it
/// clears within milliseconds.
fn spawn(command: &mut Command) -> Option<Child> {
    for _ in 0..5 {
        match command.spawn() {
            Ok(child) => return Some(child),
            Err(e) if e.kind() == ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                eprintln!("SPAWN ERR {e:?}");
                return None;
            }
        }
    }
    None
}

/// Runs `command` with no stdin and no stderr, returning its stdout if it
/// finished within `timeout`. `None` when it could not start, ran out of
/// time (it is killed), or printed something that is not UTF-8. A non-zero
/// exit still returns `Some` with `success: false` -- `ps` exits non-zero
/// when one of the pids it was asked about is gone, yet still answers for
/// the others.
///
/// Blocks the calling thread for up to `timeout`: call it from a
/// background thread only, never from a window or panel callback (see
/// toast.rs's note on the AppKit main-thread rule).
pub fn run(mut command: Command, timeout: Duration) -> Option<Captured> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = spawn(&mut command)?;
    let stdout = child.stdout.take()?;

    // Drained on its own thread so a large answer can't fill the pipe and
    // block the child against the wait below. It hands over what it reads
    // as it goes, so the child's exit, not EOF, ends the wait: the pipe's
    // write end can be held a moment longer by a process forked elsewhere
    // at the same time.
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stdout = stdout.take(MAX_OUTPUT);
        let mut chunk = [0u8; 8192];
        while let Ok(n) = stdout.read(&mut chunk) {
            if n == 0 || tx.send(chunk[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    let deadline = Instant::now() + timeout;
    let success = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };

    // The child is gone, so everything it wrote is already in the pipe:
    // collect it until EOF, or until the reader goes quiet.
    let mut buf = Vec::new();
    while let Ok(chunk) = rx.recv_timeout(Duration::from_millis(50)) {
        buf.extend(chunk);
    }
    Some(Captured {
        success,
        stdout: String::from_utf8(buf).ok()?,
    })
}
