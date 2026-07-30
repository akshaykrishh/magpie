// The `kill -9` backstop for lease recovery. The graceful path (an MCP
// server releasing its own leases when its host disconnects) is in
// magpie-mcp; this covers the case that path can't run at all -- a session
// killed abruptly enough that it never got to release anything. Runs once
// at GUI startup rather than on a timer: leases don't expire, so there's no
// need to poll for staleness, only to clean up whatever was left over from
// before this process started (see docs/design.md "MCP contract" for why
// leases have no auto-expiry at all).

use magpie_core::Store;

pub fn sweep(store: &Store) {
    let leases = match store.list_active_leases() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("magpie: could not list active leases for dead-pid sweep: {e}");
            return;
        }
    };

    for (capture_id, pid) in leases {
        if !process_is_alive(pid) {
            if let Err(e) = store.release_lease(capture_id) {
                eprintln!("magpie: failed to release dead lease on capture {capture_id}: {e}");
            } else {
                eprintln!(
                    "magpie: released lease on capture {capture_id} -- holding process {pid} is gone"
                );
            }
        }
    }
}

#[cfg(unix)]
fn process_is_alive(pid: i64) -> bool {
    // Signal 0 sends nothing -- it only checks whether the target process
    // exists, which is exactly the liveness check this needs without
    // actually affecting the process. A nonzero return doesn't always mean
    // "dead", though: EPERM means the process exists but is owned by
    // another user, so only ESRCH ("no such process") counts as dead --
    // anything else, including a permissions error, means the process is
    // still there.
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(not(unix))]
fn process_is_alive(_pid: i64) -> bool {
    // No Windows story yet (deferred by design, see docs/design.md); treat
    // every lease as alive rather than guessing wrong and releasing work
    // that's still genuinely in progress.
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_is_alive() {
        assert!(process_is_alive(std::process::id() as i64));
    }

    #[test]
    fn a_pid_that_cannot_exist_is_not_alive() {
        // PIDs are 16-bit on most systems in practice; this is far outside
        // any real range and shouldn't correspond to a running process.
        assert!(!process_is_alive(i32::MAX as i64));
    }
}
