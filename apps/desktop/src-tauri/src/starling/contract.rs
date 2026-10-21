// What starling's two JSON commands return (schema 1; see
// docs/integration.md), and the rule for when to believe them. Anything
// short of a well-formed schema-1 answer is "starling is not installed",
// never an error: the feature is simply absent.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::run;

const SCHEMA: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Working,
    Waiting,
    Idle,
    Active,
    Ended,
    /// A state a newer starling added: kept so one new value does not make
    /// the whole feature disappear, and treated as not live.
    #[serde(other)]
    Unknown,
}

impl SessionState {
    /// Whether the agent is running right now. `idle` is starling's
    /// "transcript went quiet", which can be a session closed long ago, so
    /// it does not count as a live agent.
    pub fn is_live(self) -> bool {
        matches!(self, Self::Working | Self::Waiting | Self::Active)
    }
}

/// One entry of `starling ls --json`. Only what magpie reads is declared;
/// the rest of starling's fields are ignored.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StarlingSession {
    pub id: String,
    pub cwd: String,
    pub state: SessionState,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// Epoch milliseconds, fractional in practice.
    #[serde(default)]
    pub since: Option<f64>,
    /// The `claude` process; absent when starling never learned it.
    #[serde(default)]
    pub pid: Option<i64>,
    #[serde(default)]
    pub stream: Option<String>,
}

/// One entry of `starling streams --json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StarlingStream {
    pub name: String,
    pub cwd: String,
    pub status: String,
}

impl StarlingStream {
    pub fn is_open(&self) -> bool {
        self.status == "open"
    }
}

fn parse_list<T: for<'de> Deserialize<'de>>(text: &str, key: &str) -> Option<Vec<T>> {
    let value: Value = serde_json::from_str(text).ok()?;
    if value.get("schema")?.as_u64()? != SCHEMA {
        return None;
    }
    serde_json::from_value(value.get(key)?.clone()).ok()
}

pub fn parse_sessions(text: &str) -> Option<Vec<StarlingSession>> {
    parse_list(text, "sessions")
}

pub fn parse_streams(text: &str) -> Option<Vec<StarlingStream>> {
    parse_list(text, "streams")
}

/// `starling <args>` under the call timeout, parsed with `parse`.
///
/// The binary's own directory is put on the child's PATH first: an npm
/// global starts with `#!/usr/bin/env node`, and `node` lives next to it
/// under nvm, but a GUI app's PATH has neither.
fn call<T>(
    binary: &Path,
    args: &[&str],
    timeout: Duration,
    parse: fn(&str) -> Option<T>,
) -> Option<T> {
    let mut command = Command::new(binary);
    command.args(args);
    if let Some(dir) = binary.parent() {
        let mut dirs = vec![dir.to_path_buf()];
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path));
        }
        if let Ok(joined) = std::env::join_paths(dirs) {
            command.env("PATH", joined);
        }
    }
    let answer = run::run(command, timeout)?;
    if !answer.success {
        return None;
    }
    parse(&answer.stdout)
}

pub fn fetch_sessions(binary: &Path, timeout: Duration) -> Option<Vec<StarlingSession>> {
    call(binary, &["ls", "--json"], timeout, parse_sessions)
}

pub fn fetch_streams(binary: &Path, timeout: Duration) -> Option<Vec<StarlingStream>> {
    call(binary, &["streams", "--json"], timeout, parse_streams)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LS: &str = r#"{"schema":1,"sessions":[
        {"id":"a","profile":"work","cwd":"/r/shop","branch":"refunds","title":"Partial refunds",
         "state":"waiting","since":1730000000000.5,"updated":1730000000000,"pid":4242,"stream":"billing-retry"},
        {"id":"b","cwd":"/r/other","state":"active"}]}"#;

    #[test]
    fn parses_sessions_and_tolerates_missing_optionals() {
        let sessions = parse_sessions(LS).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].state, SessionState::Waiting);
        assert_eq!(sessions[0].pid, Some(4242));
        assert_eq!(sessions[0].stream.as_deref(), Some("billing-retry"));
        assert_eq!(sessions[1].pid, None);
        assert_eq!(sessions[1].title, None);
    }

    #[test]
    fn a_state_from_a_newer_starling_is_kept_but_not_live() {
        let text = r#"{"schema":1,"sessions":[{"id":"a","cwd":"/r","state":"snoozing"}]}"#;
        let sessions = parse_sessions(text).unwrap();
        assert_eq!(sessions[0].state, SessionState::Unknown);
        assert!(!sessions[0].state.is_live());
    }

    #[test]
    fn only_working_waiting_and_active_are_live() {
        use SessionState::*;
        let live: Vec<_> = [Working, Waiting, Idle, Active, Ended]
            .into_iter()
            .filter(|s| s.is_live())
            .collect();
        assert_eq!(live, [Working, Waiting, Active]);
    }

    #[test]
    fn anything_but_a_schema_1_document_is_absent() {
        for text in [
            "",
            "not json",
            "[]",
            r#"{"sessions":[]}"#,
            r#"{"schema":2,"sessions":[]}"#,
            r#"{"schema":"1","sessions":[]}"#,
            r#"{"schema":1}"#,
            r#"{"schema":1,"sessions":"nope"}"#,
            r#"{"schema":1,"sessions":[{"cwd":"/r","state":"idle"}]}"#,
        ] {
            assert_eq!(parse_sessions(text), None, "{text}");
        }
    }

    #[test]
    fn parses_streams_and_their_status() {
        let text = r#"{"schema":1,"streams":[
            {"name":"billing-retry","cwd":"/r/shop","profile":"work","status":"open","created":1,"updated":2},
            {"name":"old","cwd":"/r/shop","status":"closed"}]}"#;
        let streams = parse_streams(text).unwrap();
        assert_eq!(streams.len(), 2);
        assert!(streams[0].is_open());
        assert!(!streams[1].is_open());
        assert_eq!(parse_streams(r#"{"schema":3,"streams":[]}"#), None);
    }
}

#[cfg(all(test, unix))]
mod fetch_tests {
    use super::*;
    use crate::starling::testing::{fake_starling, script, TempDir};

    const GOOD: &str = r#"{"schema":1,"sessions":[{"id":"a","cwd":"/r","state":"working"}]}"#;

    /// Generous: the first run of a freshly written script can be slow
    /// under a loaded machine, and these tests are about the answer, not
    /// the clock (the one about the clock sets its own).
    const PATIENT: Duration = Duration::from_secs(20);

    fn sessions(bin: &Path) -> Option<Vec<StarlingSession>> {
        fetch_sessions(bin, PATIENT)
    }

    #[test]
    fn a_working_starling_is_read() {
        let tmp = TempDir::new();
        let bin = fake_starling(tmp.path(), Some(GOOD), Some(r#"{"schema":1,"streams":[]}"#));
        assert_eq!(sessions(&bin).unwrap().len(), 1);
        assert_eq!(fetch_streams(&bin, PATIENT).unwrap().len(), 0);
    }

    #[test]
    fn a_non_zero_exit_is_absent_even_with_valid_output() {
        let tmp = TempDir::new();
        let bin = script(&tmp.join("starling"), &format!("echo '{GOOD}'\nexit 3"));
        assert_eq!(sessions(&bin), None);
    }

    #[test]
    fn non_json_and_other_schemas_are_absent() {
        let tmp = TempDir::new();
        let bin = fake_starling(tmp.path(), Some("starling 1.2.3"), None);
        assert_eq!(sessions(&bin), None);
        let bin = fake_starling(tmp.path(), Some(r#"{"schema":2,"sessions":[]}"#), None);
        assert_eq!(sessions(&bin), None);
    }

    #[test]
    fn a_missing_binary_is_absent() {
        let tmp = TempDir::new();
        assert_eq!(sessions(&tmp.join("no-such-starling")), None);
    }

    #[test]
    fn a_hung_starling_is_cut_off_at_the_call_timeout() {
        let tmp = TempDir::new();
        let bin = script(&tmp.join("starling"), "exec sleep 30");
        let started = std::time::Instant::now();
        assert_eq!(fetch_sessions(&bin, Duration::from_millis(300)), None);
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn the_binarys_own_directory_is_on_its_path() {
        // An npm global's `#!/usr/bin/env node` finds node next to it under
        // nvm; a GUI app's own PATH has no such directory.
        let tmp = TempDir::new();
        script(&tmp.join("bin/sibling-tool"), &format!("echo '{GOOD}'"));
        let bin = script(&tmp.join("bin/starling"), "sibling-tool");
        assert_eq!(sessions(&bin).unwrap().len(), 1);
    }

    #[test]
    fn a_large_answer_does_not_deadlock_the_pipe() {
        let tmp = TempDir::new();
        let entry = r#"{"id":"a","cwd":"/r","state":"idle","title":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"}"#;
        let many = vec![entry; 3000].join(",");
        let bin = fake_starling(
            tmp.path(),
            Some(&format!(r#"{{"schema":1,"sessions":[{many}]}}"#)),
            None,
        );
        assert_eq!(sessions(&bin).unwrap().len(), 3000);
    }
}
