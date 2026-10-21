// The "magpie reads starling" half of docs/integration.md: when starling
// is installed, its sessions and streams show up next to magpie's own.
// When it is not, or it answers with anything but a schema-1 document,
// nothing here is visible -- no error, no toast.
//
// Starling is a subprocess (a node script, a few hundred milliseconds), so
// no window ever waits on it. Commands read the last snapshot from memory;
// `touch` asks for a fresh one on a background thread, at most once per
// `MIN_INTERVAL`, and a `starling:changed` event tells the windows when it
// differs, so a new agent appears without waiting for the next poll.

mod agents;
mod contract;
mod locate;
mod run;
#[cfg(test)]
mod testing;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use magpie_core::Store;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

pub use agents::branch_constraint;
pub use agents::{
    augment_overview, compose, project_lookup, stream_destinations, AgentRow, AgentsView,
    ProjectOverviewView, StreamDestination,
};
use agents::{parse_ps_parents, Place, Snapshot, StreamEntry};
pub use contract::SessionState;
use locate::Locator;

/// The branch checked out in `dir` right now; see `Place::current_branch`.
pub fn current_branch(dir: &Path) -> Option<Option<String>> {
    Place::current_branch(dir)
}

pub const CHANGED_EVENT: &str = "starling:changed";

/// The floor on how often starling is asked for sessions: the same 5s the
/// Dock's own poll runs at, shared by every window and the capture hotkey,
/// so several windows polling still cost one call.
const MIN_INTERVAL: Duration = Duration::from_secs(5);
/// Streams change by hand, not by the minute.
const STREAMS_TTL: Duration = Duration::from_secs(30);
/// A folder's git facts barely change; this is what keeps a dozen sessions
/// from costing a dozen git calls every five seconds.
const PLACE_TTL: Duration = Duration::from_secs(60);
/// A repository's default branch almost never changes.
const DEFAULT_BRANCH_TTL: Duration = Duration::from_secs(600);
/// How long a transient failure keeps the last good answer on screen: two
/// polls, so one timeout does not make every row blink out and back.
const GRACE: Duration = Duration::from_secs(10);
/// With no window showing nobody is looking, so ask this rarely.
const HIDDEN_INTERVAL: Duration = Duration::from_secs(30);
/// After "not found" or a bad answer, how long before trying again on its
/// own (a window regaining focus can ask sooner).
const RETRY_AFTER: Duration = Duration::from_secs(30);

enum Binary {
    Unresolved,
    Found(PathBuf),
    /// Cached like a hit, so a machine without starling does not search for
    /// it on every poll.
    Missing(Instant),
}

struct Inner {
    binary: Binary,
    snapshot: Option<Arc<Snapshot>>,
    fetched: Option<Instant>,
    /// After a failed call; `touch` waits this out.
    retry_at: Option<Instant>,
    streams: Option<(Instant, Vec<StreamEntry>)>,
    places: HashMap<String, (Instant, Place)>,
    /// Per folder, including "could not tell": resolving may ask the remote,
    /// which is too slow and too chatty to repeat with every streams refresh.
    default_branches: HashMap<String, (Instant, Option<String>)>,
}

impl Inner {
    /// Whether a fresh snapshot is wanted: the last one is older than
    /// `interval`, and a recent failure is not still being waited out.
    fn due(&self, now: Instant, interval: Duration) -> bool {
        let fresh = self.fetched.is_some_and(|t| now < t + interval);
        let backing_off = self.retry_at.is_some_and(|t| now < t);
        !fresh && !backing_off
    }
}

pub struct Starling {
    inner: Mutex<Inner>,
    refreshing: AtomicBool,
    /// How long each call to starling (and `ps`) may take.
    call_timeout: Duration,
    ask_remote: bool,
}

impl Starling {
    pub fn new() -> Self {
        Self::build(run::CALL_TIMEOUT, true)
    }

    #[cfg(test)]
    fn with_call_timeout(call_timeout: Duration) -> Self {
        Self::build(call_timeout, false)
    }

    /// `ask_remote`: whether finding a default branch may ask `origin`
    /// (`git ls-remote`). Off in tests, which must not reach the network.
    fn build(call_timeout: Duration, ask_remote: bool) -> Self {
        Self {
            inner: Mutex::new(Inner {
                binary: Binary::Unresolved,
                snapshot: None,
                fetched: None,
                retry_at: None,
                streams: None,
                places: HashMap::new(),
                default_branches: HashMap::new(),
            }),
            refreshing: AtomicBool::new(false),
            call_timeout,
            ask_remote,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("starling mutex poisoned")
    }

    /// The last good answer, or `None` while starling is absent. Never
    /// blocks beyond the mutex.
    pub fn snapshot(&self) -> Option<Arc<Snapshot>> {
        self.lock().snapshot.clone()
    }

    /// Asks for a fresh snapshot in the background if the current one is
    /// older than `MIN_INTERVAL`. Cheap and safe to call from anywhere,
    /// including a command running on the main thread: it never waits.
    pub fn touch(self: &Arc<Self>, app: &AppHandle) {
        {
            let inner = self.lock();
            if matches!(inner.binary, Binary::Missing(_)) {
                return;
            }
            let any_window_shown = app
                .webview_windows()
                .values()
                .any(|w| w.is_visible().unwrap_or(false));
            let interval = if any_window_shown {
                MIN_INTERVAL
            } else {
                HIDDEN_INTERVAL
            };
            if !inner.due(Instant::now(), interval) {
                return;
            }
        }
        if self.refreshing.swap(true, Ordering::SeqCst) {
            return;
        }
        let this = Arc::clone(self);
        let app = app.clone();
        std::thread::spawn(move || {
            // Cleared however this ends, a panic included: a refresh that
            // never reset the flag would silence starling until restart.
            let _guard = RefreshGuard(Arc::clone(&this));
            let state = app.state::<AppState>();
            if this.refresh(&Locator::from_process(), &state.store) {
                let _ = app.emit(CHANGED_EVENT, ());
            }
        });
    }

    /// A window regained focus: forget a stale "not found" (starling may
    /// have been installed since) or a binary that has gone, then ask for a
    /// snapshot. Cheap: the only work is a clock read unless something
    /// actually changed.
    pub fn recheck(self: &Arc<Self>, app: &AppHandle) {
        self.forget_stale_binary();
        self.touch(app);
    }

    fn forget_stale_binary(&self) {
        let mut inner = self.lock();
        let stale = match &inner.binary {
            Binary::Missing(at) => at.elapsed() >= RETRY_AFTER,
            Binary::Found(path) => !path.exists(),
            Binary::Unresolved => false,
        };
        if stale {
            inner.binary = Binary::Unresolved;
            inner.retry_at = None;
        }
    }

    /// One full refresh: find the binary, ask it, gather the process-tree
    /// and git facts the merge needs. Returns whether what the windows see
    /// changed. Blocks for as long as the calls take (each bounded by
    /// `run::CALL_TIMEOUT`): background threads only.
    fn refresh(&self, locator: &Locator, store: &Store) -> bool {
        let Some(binary) = self.binary(locator) else {
            return self.publish(None);
        };
        let Some(sessions) = contract::fetch_sessions(&binary, self.call_timeout) else {
            let mut inner = self.lock();
            let now = Instant::now();
            // A blip: keep showing the last good answer for a grace period
            // and look again at the next poll. Past it, starling is treated
            // as gone and checked on the slow schedule.
            if inner.fetched.is_some_and(|t| now < t + GRACE) {
                inner.retry_at = Some(now + MIN_INTERVAL);
                return false;
            }
            inner.retry_at = Some(now + RETRY_AFTER);
            drop(inner);
            return self.publish(None);
        };

        let streams = self.streams(&binary);
        let live_pids: Vec<i64> = store
            .list_active_sessions()
            .unwrap_or_default()
            .into_iter()
            .map(|(_, pid)| pid)
            .collect();
        let parents = parent_pids(&live_pids, self.call_timeout);

        let cwds = sessions
            .iter()
            .filter(|s| s.state.is_live())
            .map(|s| s.cwd.as_str())
            .chain(streams.iter().map(|s| s.cwd.as_str()));
        let places = self.places(cwds);

        let mut inner = self.lock();
        inner.fetched = Some(Instant::now());
        inner.retry_at = None;
        drop(inner);
        self.publish(Some(Snapshot {
            sessions,
            parents,
            places,
            streams,
        }))
    }

    /// Stores `snapshot`, reporting whether it differs from the last one.
    fn publish(&self, snapshot: Option<Snapshot>) -> bool {
        let mut inner = self.lock();
        if snapshot.is_none() {
            inner.fetched = None;
        }
        let changed = inner.snapshot.as_deref() != snapshot.as_ref();
        if changed {
            inner.snapshot = snapshot.map(Arc::new);
        }
        changed
    }

    fn binary(&self, locator: &Locator) -> Option<PathBuf> {
        {
            let inner = self.lock();
            match &inner.binary {
                Binary::Found(path) => return Some(path.clone()),
                Binary::Missing(_) => return None,
                Binary::Unresolved => {}
            }
        }
        // Resolved without the lock held: the login-shell step can take a
        // moment and `snapshot()` must stay instant meanwhile.
        let found = locator.locate();
        self.lock().binary = match &found {
            Some(path) => Binary::Found(path.clone()),
            None => Binary::Missing(Instant::now()),
        };
        found
    }

    fn streams(&self, binary: &Path) -> Vec<StreamEntry> {
        {
            let inner = self.lock();
            if let Some((at, cached)) = &inner.streams {
                if at.elapsed() < STREAMS_TTL {
                    return cached.clone();
                }
            }
        }
        // A starling whose `streams` fails still has sessions worth
        // showing, so this degrades to "no streams", not to absent.
        let entries: Vec<StreamEntry> = contract::fetch_streams(binary, self.call_timeout)
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.is_open())
            .map(|s| StreamEntry {
                default_branch: self.default_branch(&s.cwd),
                name: s.name,
                cwd: s.cwd,
            })
            .collect();
        self.lock().streams = Some((Instant::now(), entries.clone()));
        entries
    }

    fn default_branch(&self, cwd: &str) -> Option<String> {
        let cached = self
            .lock()
            .default_branches
            .get(cwd)
            .filter(|(at, _)| at.elapsed() < DEFAULT_BRANCH_TTL)
            .map(|(_, branch)| branch.clone());
        cached.unwrap_or_else(|| {
            let branch = default_branch(Path::new(cwd), self.call_timeout, self.ask_remote);
            self.lock()
                .default_branches
                .insert(cwd.to_string(), (Instant::now(), branch.clone()));
            branch
        })
    }

    fn places<'a>(&self, cwds: impl Iterator<Item = &'a str>) -> HashMap<String, Place> {
        let mut out = HashMap::new();
        for cwd in cwds {
            if out.contains_key(cwd) {
                continue;
            }
            let cached = self
                .lock()
                .places
                .get(cwd)
                .filter(|(at, _)| at.elapsed() < PLACE_TTL)
                .map(|(_, place)| place.clone());
            let place = cached.unwrap_or_else(|| {
                let place = Place::of(Path::new(cwd));
                self.lock()
                    .places
                    .insert(cwd.to_string(), (Instant::now(), place.clone()));
                place
            });
            out.insert(cwd.to_string(), place);
        }
        out
    }

    /// The Dock's agents: empty, and `present: false`, without starling.
    pub fn agents(&self, store: &Store) -> AgentsView {
        let Some(snapshot) = self.snapshot() else {
            return AgentsView::default();
        };
        AgentsView {
            present: true,
            agents: compose_for(&snapshot, store),
        }
    }

    /// Open streams that can take a capture, for the aim picker. Empty
    /// without starling.
    pub fn stream_destinations(&self, store: &Store) -> Vec<StreamDestination> {
        match self.snapshot() {
            Some(snapshot) => stream_destinations(&snapshot, &project_lookup(store)),
            None => Vec::new(),
        }
    }
}

/// Clears `Starling::refreshing` when dropped.
struct RefreshGuard(Arc<Starling>);

impl Drop for RefreshGuard {
    fn drop(&mut self) {
        self.0.refreshing.store(false, Ordering::SeqCst);
    }
}

fn compose_for(snapshot: &Snapshot, store: &Store) -> Vec<AgentRow> {
    let live: Vec<_> = store
        .list_sessions(None)
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.ended_at.is_none())
        .collect();
    compose(snapshot, &live, &project_lookup(store))
}

/// pid -> parent pid for those of `pids` that are still magpie's own MCP
/// server, through `ps`. A pid that has gone, or that now belongs to some
/// other program, is missing from the answer (and makes `ps` exit non-zero, which
/// `run` reports without discarding the output).
fn parent_pids(pids: &[i64], timeout: Duration) -> HashMap<i64, i64> {
    if pids.is_empty() {
        return HashMap::new();
    }
    let list = pids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let mut command = Command::new("ps");
    command.args(["-o", "pid=,ppid=,comm=", "-p", &list]);
    run::run(command, timeout)
        .map(|out| parse_ps_parents(&out.stdout))
        .unwrap_or_default()
}

/// The repository's default branch, resolved without guessing: what
/// `origin/HEAD` points at; else what the remote itself says HEAD is (a
/// network call, so only when `ask_remote` and there is an `origin`,
/// bounded by `timeout` and cached by the caller); else the configured `init.defaultBranch`, then
/// `main`, then `master` -- each only if that branch exists here. `None`
/// when none of that settles it, so nothing gets pinned to a guess.
fn default_branch(dir: &Path, timeout: Duration, ask_remote: bool) -> Option<String> {
    resolve_default_branch(ask_remote, &|args| {
        let mut command = Command::new("git");
        command.current_dir(dir).args(args);
        // Never wait on a credential prompt or an ssh host-key question.
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
        let out = run::run(command, timeout).filter(|o| o.success)?;
        Some(out.stdout.trim().to_string()).filter(|text| !text.is_empty())
    })
}

/// `default_branch` over any way of running git (its trimmed non-empty
/// stdout, or `None` on failure), so the order of the fallbacks can be
/// tested without a repository.
fn resolve_default_branch(
    ask_remote: bool,
    git: &dyn Fn(&[&str]) -> Option<String>,
) -> Option<String> {
    let name = |full: &str, prefix: &str| {
        full.strip_prefix(prefix)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
    };
    if let Some(branch) = git(&["symbolic-ref", "refs/remotes/origin/HEAD"])
        .and_then(|r| name(&r, "refs/remotes/origin/"))
    {
        return Some(branch);
    }
    if ask_remote && git(&["remote", "get-url", "origin"]).is_some() {
        // "ref: refs/heads/develop\tHEAD" on the first line.
        let asked = git(&["ls-remote", "--symref", "origin", "HEAD"]).and_then(|out| {
            out.lines().find_map(|line| {
                let target = line.strip_prefix("ref:")?.split_whitespace().next()?;
                name(target, "refs/heads/")
            })
        });
        if asked.is_some() {
            return asked;
        }
    }
    let exists = |branch: &str| {
        git(&[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .is_some()
            || git(&[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/remotes/origin/{branch}"),
            ])
            .is_some()
    };
    git(&["config", "--get", "init.defaultBranch"])
        .into_iter()
        .chain(["main", "master"].map(str::to_string))
        .find(|branch| exists(branch))
}

#[cfg(all(test, unix))]
mod tests {
    use super::testing::{fake_starling, TempDir};
    use super::*;

    const REMOTE: &str = "git@github.com:acme/shop-api.git";
    /// Generous for the same reason as contract.rs's tests: a freshly
    /// written script can be slow to run the first time.
    const PATIENT: Duration = Duration::from_secs(20);

    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    }

    /// A checkout of shop-api on branch `refunds`.
    fn repo(tmp: &TempDir) -> PathBuf {
        let dir = tmp.join("shop-api");
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "refunds"]);
        git(&dir, &["remote", "add", "origin", REMOTE]);
        git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
        // The default branch, so `refunds` reads as a feature branch.
        git(&dir, &["branch", "main"]);
        dir
    }

    fn locator_for(bin: &Path) -> Locator {
        Locator {
            override_bin: Some(bin.as_os_str().to_owned()),
            path: None,
            shell: None,
            home: None,
            fixed_dirs: Vec::new(),
            shell_timeout: Duration::from_secs(1),
        }
    }

    fn ls_json(cwd: &Path, claude_pid: u32) -> String {
        format!(
            r#"{{"schema":1,"sessions":[
            {{"id":"s-matched","cwd":"{cwd}","branch":"refunds","title":"Partial refunds","state":"waiting","since":1730000000000,"pid":{claude_pid}}},
            {{"id":"s-bare","cwd":"{cwd}","title":"Audit log","state":"working","since":1730000001000,"pid":999999}},
            {{"id":"s-old","cwd":"{cwd}","state":"idle","pid":999998}}]}}"#,
            cwd = cwd.display()
        )
    }

    fn streams_json(cwd: &Path) -> String {
        format!(
            r#"{{"schema":1,"streams":[
            {{"name":"billing-retry","cwd":"{cwd}","status":"open"}},
            {{"name":"done-long-ago","cwd":"{cwd}","status":"closed"}}]}}"#,
            cwd = cwd.display()
        )
    }

    #[test]
    fn a_refresh_matches_through_the_real_process_tree_and_finds_streams() {
        let tmp = TempDir::new();
        let repo = repo(&tmp);

        let store = Store::open_in_memory().unwrap();
        let project = store
            .get_or_create_project("shop-api", Some(REMOTE), None)
            .unwrap();
        // This test process plays `claude`; its child plays magpie's MCP
        // server, exactly the relationship docs/integration.md describes.
        // It has to be called `magpie`: only magpie's own binary counts.
        let magpie_bin = tmp.join("magpie");
        std::os::unix::fs::symlink("/bin/sleep", &magpie_bin).unwrap();
        let mut mcp_server = Command::new(&magpie_bin).arg("30").spawn().unwrap();
        store
            .create_session(
                "m1",
                mcp_server.id() as i64,
                Some(project.id),
                Some("refunds"),
            )
            .unwrap();

        let bin = fake_starling(
            tmp.path(),
            Some(&ls_json(&repo, std::process::id())),
            Some(&streams_json(&repo)),
        );
        let starling = Starling::with_call_timeout(PATIENT);
        let changed = starling.refresh(&locator_for(&bin), &store);
        // The same answer again changes nothing, so no event goes out.
        let changed_again = starling.refresh(&locator_for(&bin), &store);
        let _ = mcp_server.kill();
        let _ = mcp_server.wait();

        assert!(changed);
        assert!(!changed_again);
        let view = starling.agents(&store);
        assert!(view.present);
        let rows: Vec<_> = view
            .agents
            .iter()
            .map(|r| {
                (
                    r.starling_session.as_str(),
                    r.magpie_session.as_deref(),
                    r.state,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                ("s-matched", Some("m1"), contract::SessionState::Waiting),
                ("s-bare", None, contract::SessionState::Working),
            ],
            "the idle session is not an agent, and the bare one has no magpie session"
        );
        assert!(view.agents.iter().all(|r| r.project_id == project.id));

        let streams = starling.stream_destinations(&store);
        assert_eq!(streams.len(), 1, "closed streams are not offered");
        assert_eq!(streams[0].stream, "billing-retry");
        assert_eq!(streams[0].project_id, project.id);
        assert_eq!(streams[0].branch.as_deref(), Some("refunds"));
    }

    #[test]
    fn a_stream_on_the_default_branch_carries_no_branch_constraint() {
        let tmp = TempDir::new();
        let repo = repo(&tmp);
        git(&repo, &["checkout", "-q", "main"]);
        let store = Store::open_in_memory().unwrap();
        store
            .get_or_create_project("shop-api", Some(REMOTE), None)
            .unwrap();
        let bin = fake_starling(
            tmp.path(),
            Some(r#"{"schema":1,"sessions":[]}"#),
            Some(&streams_json(&repo)),
        );
        let starling = Starling::with_call_timeout(PATIENT);
        starling.refresh(&locator_for(&bin), &store);
        assert_eq!(starling.stream_destinations(&store)[0].branch, None);
    }

    #[test]
    fn no_binary_means_absent_and_the_miss_is_cached() {
        let tmp = TempDir::new();
        let store = Store::open_in_memory().unwrap();
        let bin = tmp.join("starling");

        let starling = Starling::with_call_timeout(PATIENT);
        assert!(!starling.refresh(&locator_for(&bin), &store));
        assert!(!starling.agents(&store).present);
        assert!(starling.stream_destinations(&store).is_empty());

        // Installed afterwards: still absent, the miss was remembered...
        fake_starling(tmp.path(), Some(r#"{"schema":1,"sessions":[]}"#), None);
        assert!(!starling.refresh(&locator_for(&bin), &store));
        assert!(!starling.agents(&store).present);

        // ...until a focus-time recheck finds the miss old enough to retry.
        starling.forget_stale_binary();
        assert!(
            !starling.agents(&store).present,
            "a fresh miss is not retried"
        );
        starling.lock().binary = Binary::Missing(Instant::now() - RETRY_AFTER);
        starling.forget_stale_binary();
        starling.refresh(&locator_for(&bin), &store);
        assert!(starling.agents(&store).present);
    }

    #[test]
    fn a_binary_that_has_gone_is_looked_for_again() {
        let tmp = TempDir::new();
        let store = Store::open_in_memory().unwrap();
        let bin = fake_starling(tmp.path(), Some(r#"{"schema":1,"sessions":[]}"#), None);
        let starling = Starling::with_call_timeout(PATIENT);
        starling.refresh(&locator_for(&bin), &store);
        assert!(starling.agents(&store).present);

        std::fs::remove_file(&bin).unwrap();
        starling.forget_stale_binary();
        assert!(matches!(starling.lock().binary, Binary::Unresolved));
    }

    #[test]
    fn a_bad_answer_is_absent_never_an_error() {
        let store = Store::open_in_memory().unwrap();
        for ls in [
            Some("starling 1.2.3"),
            Some(r#"{"schema":2,"sessions":[]}"#),
            Some(r#"{"sessions":[]}"#),
            Some("{"),
            None,
        ] {
            let tmp = TempDir::new();
            let bin = fake_starling(tmp.path(), ls, if ls.is_none() { None } else { Some("") });
            let starling = Starling::with_call_timeout(PATIENT);
            starling.refresh(&locator_for(&bin), &store);
            assert!(!starling.agents(&store).present, "{ls:?}");
            assert!(
                starling.lock().retry_at.is_some(),
                "a bad answer backs off, {ls:?}"
            );
        }
    }

    #[test]
    fn one_bad_answer_keeps_the_last_good_one_for_a_grace_period() {
        let tmp = TempDir::new();
        let store = Store::open_in_memory().unwrap();
        let bin = fake_starling(tmp.path(), Some(r#"{"schema":1,"sessions":[]}"#), None);
        let starling = Starling::with_call_timeout(PATIENT);
        starling.refresh(&locator_for(&bin), &store);

        fake_starling(tmp.path(), Some("starling is having a moment"), None);
        assert!(
            !starling.refresh(&locator_for(&bin), &store),
            "nothing for the windows to hear"
        );
        assert!(
            starling.agents(&store).present,
            "rows do not blink out on one failure"
        );
        assert!(starling.lock().retry_at.is_some());
    }

    #[test]
    fn a_refresh_is_due_on_the_interval_and_not_while_backing_off() {
        let starling = Starling::new();
        let now = Instant::now();
        let mut inner = starling.lock();
        assert!(inner.due(now, MIN_INTERVAL), "nothing fetched yet");

        inner.fetched = Some(now);
        assert!(!inner.due(now + Duration::from_secs(1), MIN_INTERVAL));
        assert!(inner.due(now + MIN_INTERVAL, MIN_INTERVAL));
        assert!(
            !inner.due(now + MIN_INTERVAL, HIDDEN_INTERVAL),
            "with no window showing it waits longer"
        );
        assert!(inner.due(now + HIDDEN_INTERVAL, HIDDEN_INTERVAL));

        inner.retry_at = Some(now + RETRY_AFTER);
        assert!(!inner.due(now + Duration::from_secs(10), MIN_INTERVAL));
    }

    #[test]
    fn starling_going_bad_after_a_good_answer_withdraws_the_feature() {
        let tmp = TempDir::new();
        let store = Store::open_in_memory().unwrap();
        let bin = fake_starling(tmp.path(), Some(r#"{"schema":1,"sessions":[]}"#), None);
        let starling = Starling::with_call_timeout(PATIENT);
        starling.refresh(&locator_for(&bin), &store);
        assert!(starling.agents(&store).present);

        // Past the grace period, a bad answer is no longer a blip.
        starling.lock().fetched = Some(Instant::now() - GRACE - Duration::from_secs(1));
        fake_starling(tmp.path(), Some(r#"{"schema":9,"sessions":[]}"#), None);
        assert!(
            starling.refresh(&locator_for(&bin), &store),
            "a change the windows must hear about"
        );
        assert!(!starling.agents(&store).present);
    }

    #[test]
    fn the_refreshing_flag_is_cleared_even_when_a_refresh_panics() {
        let starling = Arc::new(Starling::new());
        starling.refreshing.store(true, Ordering::SeqCst);
        let held = Arc::clone(&starling);

        let outcome = std::panic::catch_unwind(move || {
            let _guard = RefreshGuard(held);
            panic!("refresh blew up");
        });

        assert!(outcome.is_err());
        assert!(!starling.refreshing.load(Ordering::SeqCst));
    }

    #[test]
    fn a_failing_streams_command_still_leaves_the_sessions() {
        let tmp = TempDir::new();
        let store = Store::open_in_memory().unwrap();
        let bin = fake_starling(
            tmp.path(),
            Some(r#"{"schema":1,"sessions":[]}"#),
            Some("not json"),
        );
        let starling = Starling::with_call_timeout(PATIENT);
        starling.refresh(&locator_for(&bin), &store);
        assert!(starling.agents(&store).present);
        assert!(starling.stream_destinations(&store).is_empty());
    }

    /// A fake git: answers each command from `answers` (matched on the
    /// arguments joined by spaces), failing everything else.
    fn fake_git<'a>(
        answers: &'a [(&'a str, &'a str)],
        asked: &'a std::cell::RefCell<Vec<String>>,
    ) -> impl Fn(&[&str]) -> Option<String> + 'a {
        move |args| {
            let key = args.join(" ");
            asked.borrow_mut().push(key.clone());
            answers
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        }
    }

    fn resolved(ask_remote: bool, answers: &[(&str, &str)]) -> (Option<String>, Vec<String>) {
        let asked = std::cell::RefCell::new(Vec::new());
        let got = resolve_default_branch(ask_remote, &fake_git(answers, &asked));
        (got, asked.into_inner())
    }

    #[test]
    fn the_default_branch_is_what_origin_head_points_at() {
        let (got, asked) = resolved(
            true,
            &[(
                "symbolic-ref refs/remotes/origin/HEAD",
                "refs/remotes/origin/develop",
            )],
        );
        assert_eq!(got.as_deref(), Some("develop"));
        assert_eq!(asked.len(), 1, "nothing else is asked once it is known");
    }

    #[test]
    fn without_origin_head_the_remote_is_asked() {
        let (got, _) = resolved(
            true,
            &[
                ("remote get-url origin", "git@host:a/b.git"),
                (
                    "ls-remote --symref origin HEAD",
                    "ref: refs/heads/trunk\tHEAD\n0123abc\tHEAD",
                ),
                // A local main exists, but the remote's answer wins.
                ("rev-parse --verify --quiet refs/heads/main", "abc"),
            ],
        );
        assert_eq!(got.as_deref(), Some("trunk"));
    }

    #[test]
    fn the_remote_is_not_asked_when_that_is_switched_off_or_there_is_no_origin() {
        let (_, asked) = resolved(false, &[]);
        assert!(!asked.iter().any(|a| a.starts_with("ls-remote")));
        let (_, asked) = resolved(true, &[]);
        assert!(!asked.iter().any(|a| a.starts_with("ls-remote")));
    }

    #[test]
    fn then_the_configured_default_if_that_branch_exists() {
        let (got, _) = resolved(
            false,
            &[
                ("config --get init.defaultBranch", "develop"),
                ("rev-parse --verify --quiet refs/heads/develop", "abc"),
                ("rev-parse --verify --quiet refs/heads/main", "abc"),
            ],
        );
        assert_eq!(got.as_deref(), Some("develop"));

        // Configured but absent here: not trusted, falls through to main.
        let (got, _) = resolved(
            false,
            &[
                ("config --get init.defaultBranch", "develop"),
                ("rev-parse --verify --quiet refs/remotes/origin/main", "abc"),
            ],
        );
        assert_eq!(got.as_deref(), Some("main"));
    }

    #[test]
    fn main_or_master_only_when_one_actually_exists() {
        let (got, _) = resolved(
            false,
            &[("rev-parse --verify --quiet refs/heads/master", "abc")],
        );
        assert_eq!(got.as_deref(), Some("master"));

        let (got, _) = resolved(false, &[]);
        assert_eq!(got, None, "unknown stays unknown, never a guess");
    }

    #[test]
    fn a_repo_whose_default_is_develop_is_not_treated_as_main() {
        let tmp = TempDir::new();
        // `origin` is a real (local) repository whose HEAD is develop.
        let origin = tmp.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "-q", "--bare", "-b", "develop"]);
        let work = tmp.join("work");
        std::fs::create_dir_all(&work).unwrap();
        git(&work, &["init", "-q", "-b", "develop"]);
        git(&work, &["commit", "-q", "--allow-empty", "-m", "init"]);
        git(
            &work,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git(&work, &["push", "-q", "origin", "develop"]);
        git(&work, &["checkout", "-q", "-b", "main"]);

        // No origin/HEAD recorded locally, so this takes the remote's word.
        assert_eq!(
            default_branch(&work, PATIENT, true).as_deref(),
            Some("develop")
        );
        // Once origin/HEAD is recorded, the answer needs no network.
        git(
            &work,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/develop",
            ],
        );
        assert_eq!(
            default_branch(&work, PATIENT, false).as_deref(),
            Some("develop")
        );
    }

    #[test]
    fn a_stream_on_a_repo_with_an_unknowable_default_pins_nothing() {
        let tmp = TempDir::new();
        let repo = tmp.join("shop-api");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "refunds"]);
        git(&repo, &["remote", "add", "origin", REMOTE]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let store = Store::open_in_memory().unwrap();
        store
            .get_or_create_project("shop-api", Some(REMOTE), None)
            .unwrap();
        let bin = fake_starling(
            tmp.path(),
            Some(r#"{"schema":1,"sessions":[]}"#),
            Some(&streams_json(&repo)),
        );
        let starling = Starling::with_call_timeout(PATIENT);
        starling.refresh(&locator_for(&bin), &store);
        assert_eq!(starling.stream_destinations(&store)[0].branch, None);
    }
}
