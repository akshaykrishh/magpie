// Hard-deletes soft-deleted captures/templates/sections older than 30 days.
// Unlike dead_pid_sweep (startup-only, correct because leases don't expire
// so there's nothing to catch up on except a previous crash), this needs to
// run on a recurring timer too: a 30-day retention window depends on real
// wall-clock time passing, and this is a tray app that can plausibly stay
// open for weeks without a restart -- startup-only would mean purge never
// fires for a long-running session. See
// docs/superpowers/specs/2026-07-31-capture-list-v2-design.md "Deletion".

use magpie_core::Store;

const RETENTION_DAYS: i64 = 30;

pub fn sweep(store: &Store) {
    match store.purge_expired(RETENTION_DAYS) {
        Ok((captures, templates, sections)) => {
            if captures + templates + sections > 0 {
                eprintln!(
                    "magpie: purged {captures} capture(s), {templates} template(s), \
                     {sections} section(s) past the {RETENTION_DAYS}-day retention window"
                );
            }
        }
        Err(e) => eprintln!("magpie: purge sweep failed: {e}"),
    }
}
