// Turning starling's view of the machine into what magpie's windows show:
// which magpie sessions are the same agent starling sees, which starling
// sessions have not called a magpie tool yet, and which open streams can
// take a capture. Everything here is a pure function of a snapshot plus
// magpie's own database, so core never learns starling exists -- the
// merge happens in this app layer, per docs/integration.md.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use magpie_core::{ProjectOverview, Session, Store};
use serde::Serialize;

use super::contract::{SessionState, StarlingSession};

/// Where a folder sits in git, resolved once per refresh on a background
/// thread (it shells out to git) and then reused: the identity magpie
/// files projects under, plus the checked-out branch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Place {
    pub remote_url: Option<String>,
    pub common_git_dir: Option<String>,
    pub branch: Option<String>,
}

impl Place {
    pub fn of(dir: &Path) -> Self {
        if !dir.is_dir() {
            return Self::default();
        }
        let detected = magpie_mcp::detect_in(Some(dir));
        Self {
            remote_url: detected.remote_url,
            common_git_dir: detected.common_git_dir,
            branch: detected.branch,
        }
    }
}

/// An open starling stream, with the facts needed to offer it as a filing
/// destination. `default_branch` is `None` when git could not say.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamEntry {
    pub name: String,
    pub cwd: String,
    pub default_branch: Option<String>,
}

/// The last good answer from starling, with the process-tree and git
/// facts gathered alongside it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub sessions: Vec<StarlingSession>,
    /// magpie MCP server pid -> its parent pid, for the live magpie
    /// sessions at refresh time.
    pub parents: HashMap<i64, i64>,
    /// Keyed by cwd: every live starling session and every open stream.
    pub places: HashMap<String, Place>,
    pub streams: Vec<StreamEntry>,
}

/// One agent in the Dock, as a row. `magpie_session` is `Some` when the
/// agent has called a magpie tool (its MCP server is a child of the
/// starling-tracked `claude` process) and `None` for a starling session
/// that magpie has not heard from yet.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentRow {
    pub project_id: i64,
    pub magpie_session: Option<String>,
    /// `S1`-style ordinal and MCP client name, for a matched row only.
    pub ordinal: Option<i64>,
    pub client: Option<String>,
    pub starling_session: String,
    pub title: Option<String>,
    pub branch: Option<String>,
    pub state: SessionState,
    /// Epoch milliseconds the session entered its current state.
    pub since: Option<i64>,
    pub stream: Option<String>,
}

/// `list_agents`'s answer. `present` is false while starling is not
/// installed (or not answering), in which case `agents` is empty and the
/// windows draw nothing for it.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentsView {
    pub present: bool,
    pub agents: Vec<AgentRow>,
}

/// `list_projects_overview`'s row with starling's share added. The core
/// rollup is flattened in untouched, so a window that does not know about
/// starling reads it exactly as before; only `active_session_count`
/// changes meaning, to include starling sessions magpie has not heard
/// from yet.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectOverviewView {
    #[serde(flatten)]
    pub overview: ProjectOverview,
    /// Agents waiting for you in this project, matched or not. Kept apart
    /// from `needs_review_count`, which stays handbacks only, so a "needs
    /// review" label never describes an agent that is merely waiting.
    pub agents_waiting_count: i64,
}

/// A stream the capture toast can file to: a project, plus the branch the
/// item should be pinned to when the stream is not on the default one.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamDestination {
    pub project_id: i64,
    pub project_name: String,
    pub stream: String,
    pub branch: Option<String>,
}

pub type ProjectLookup<'a> = dyn Fn(&Place) -> Option<(i64, String)> + 'a;

/// Looks a `Place` up among magpie's projects without creating one.
pub fn project_lookup(store: &Store) -> impl Fn(&Place) -> Option<(i64, String)> + '_ {
    |place| {
        if place.remote_url.is_none() && place.common_git_dir.is_none() {
            return None;
        }
        store
            .find_project(place.remote_url.as_deref(), place.common_git_dir.as_deref())
            .ok()
            .flatten()
            .map(|p| (p.id, p.name))
    }
}

/// Every agent starling can place in a known project, most urgent first.
///
/// A magpie session matches a starling session by process tree: magpie's
/// `pid` is its MCP server, a child of the `claude` process whose pid
/// starling records. A matched row takes its project from the magpie
/// session. A live starling session with no magpie match resolves its cwd
/// to a project the same way the MCP server does; one whose folder is not
/// a magpie project is left out, since the Dock only shows the focused
/// project's agents.
pub fn compose(
    snapshot: &Snapshot,
    live_sessions: &[Session],
    project_of: &ProjectLookup,
) -> Vec<AgentRow> {
    let by_pid: HashMap<i64, &StarlingSession> = snapshot
        .sessions
        .iter()
        .filter_map(|s| s.pid.map(|pid| (pid, s)))
        .collect();

    let mut matched: HashSet<&str> = HashSet::new();
    let mut rows = Vec::new();

    for session in live_sessions {
        let Some(project_id) = session.project_id else {
            continue;
        };
        let Some(starling) = snapshot
            .parents
            .get(&session.pid)
            .and_then(|parent| by_pid.get(parent))
            .filter(|s| s.state.is_live() || s.state == SessionState::Idle)
        else {
            continue;
        };
        matched.insert(starling.id.as_str());
        rows.push(AgentRow {
            project_id,
            magpie_session: Some(session.id.clone()),
            ordinal: session.ordinal,
            client: session.client.clone(),
            ..row_from(starling, project_id)
        });
    }

    for starling in &snapshot.sessions {
        if !starling.state.is_live() || matched.contains(starling.id.as_str()) {
            continue;
        }
        let Some((project_id, _)) = snapshot.places.get(&starling.cwd).and_then(project_of) else {
            continue;
        };
        rows.push(row_from(starling, project_id));
    }

    rows.sort_by_key(|r| (urgency(r.state), std::cmp::Reverse(r.since)));
    rows
}

fn row_from(starling: &StarlingSession, project_id: i64) -> AgentRow {
    AgentRow {
        project_id,
        magpie_session: None,
        ordinal: None,
        client: None,
        starling_session: starling.id.clone(),
        title: starling.title.clone(),
        branch: starling.branch.clone(),
        state: starling.state,
        since: starling.since.map(|ms| ms as i64),
        stream: starling.stream.clone(),
    }
}

/// Waiting first: it is the one state that wants the human.
fn urgency(state: SessionState) -> u8 {
    match state {
        SessionState::Waiting => 0,
        SessionState::Working => 1,
        SessionState::Active => 2,
        _ => 3,
    }
}

/// Adds starling's agents to Across's rollup: sessions magpie has not
/// heard from yet count as live, and every waiting agent is counted
/// separately as waiting on the human.
pub fn augment_overview(
    overviews: Vec<ProjectOverview>,
    agents: &[AgentRow],
) -> Vec<ProjectOverviewView> {
    overviews
        .into_iter()
        .map(|mut overview| {
            let mut waiting = 0;
            if let Some(project_id) = overview.project_id {
                for agent in agents.iter().filter(|a| a.project_id == project_id) {
                    if agent.magpie_session.is_none() {
                        overview.active_session_count += 1;
                    }
                    if agent.state == SessionState::Waiting {
                        waiting += 1;
                    }
                }
            }
            ProjectOverviewView {
                overview,
                agents_waiting_count: waiting,
            }
        })
        .collect()
}

/// The branch to pin an item to when filing to a stream: the stream's
/// checked-out branch, unless that is the project's default (an item for
/// the default branch is just a project item, and pinning it would hide it
/// from sessions on other branches). With no answer from git about the
/// default, `main` and `master` stand in for it.
pub fn branch_constraint(branch: Option<&str>, default_branch: Option<&str>) -> Option<String> {
    let branch = branch.filter(|b| !b.is_empty())?;
    let is_default = match default_branch {
        Some(default) => branch == default,
        None => matches!(branch, "main" | "master"),
    };
    (!is_default).then(|| branch.to_string())
}

/// Open streams whose folder is a known magpie project, in starling's
/// order (most recently updated first).
pub fn stream_destinations(
    snapshot: &Snapshot,
    project_of: &ProjectLookup,
) -> Vec<StreamDestination> {
    snapshot
        .streams
        .iter()
        .filter_map(|stream| {
            let place = snapshot.places.get(&stream.cwd)?;
            let (project_id, project_name) = project_of(place)?;
            Some(StreamDestination {
                project_id,
                project_name,
                stream: stream.name.clone(),
                branch: branch_constraint(
                    place.branch.as_deref(),
                    stream.default_branch.as_deref(),
                ),
            })
        })
        .collect()
}

/// `ps -o pid=,ppid=` output as pid -> parent pid. Lines that are not two
/// numbers (a header, noise) are skipped.
pub fn parse_ps_parents(text: &str) -> HashMap<i64, i64> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let ppid = fields.next()?.parse().ok()?;
            fields.next().is_none().then_some((pid, ppid))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starling(id: &str, pid: Option<i64>, cwd: &str, state: SessionState) -> StarlingSession {
        StarlingSession {
            id: id.to_string(),
            cwd: cwd.to_string(),
            state,
            branch: Some("refunds".to_string()),
            title: Some("Partial refunds".to_string()),
            since: Some(1_730_000_000_000.5),
            pid,
            stream: None,
        }
    }

    fn magpie(id: &str, pid: i64, project_id: Option<i64>) -> Session {
        Session {
            id: id.to_string(),
            client: Some("claude-code".to_string()),
            pid,
            project_id,
            branch: None,
            started_at: "2026-10-21T10:00:00Z".to_string(),
            last_active_at: None,
            ended_at: None,
            leased_count: 0,
            completed_count: 0,
            failed_count: 0,
            handback_count: 0,
            captures_during_session: None,
            unpromoted_at_end: None,
            ordinal: Some(1),
        }
    }

    fn place(remote: &str) -> Place {
        Place {
            remote_url: Some(remote.to_string()),
            ..Default::default()
        }
    }

    /// `/shop` is project 7, `/api` is project 8, anything else unknown.
    fn snapshot(sessions: Vec<StarlingSession>, parents: &[(i64, i64)]) -> Snapshot {
        Snapshot {
            sessions,
            parents: parents.iter().copied().collect(),
            places: HashMap::from([
                ("/shop".to_string(), place("shop")),
                ("/api".to_string(), place("api")),
                ("/elsewhere".to_string(), place("elsewhere")),
            ]),
            streams: Vec::new(),
        }
    }

    fn lookup(place: &Place) -> Option<(i64, String)> {
        match place.remote_url.as_deref() {
            Some("shop") => Some((7, "shop".to_string())),
            Some("api") => Some((8, "api".to_string())),
            _ => None,
        }
    }

    #[test]
    fn a_magpie_session_matches_the_starling_session_whose_pid_is_its_parent() {
        // MCP server 5151 is a child of claude 4242.
        let snap = snapshot(
            vec![starling("s1", Some(4242), "/shop", SessionState::Waiting)],
            &[(5151, 4242)],
        );
        let rows = compose(&snap, &[magpie("m1", 5151, Some(7))], &lookup);

        assert_eq!(
            rows.len(),
            1,
            "matched, so not also listed as a bare starling row"
        );
        assert_eq!(rows[0].magpie_session.as_deref(), Some("m1"));
        assert_eq!(rows[0].starling_session, "s1");
        assert_eq!(rows[0].state, SessionState::Waiting);
        assert_eq!(rows[0].project_id, 7);
        assert_eq!(rows[0].ordinal, Some(1));
        assert_eq!(rows[0].since, Some(1_730_000_000_000));
    }

    #[test]
    fn a_matched_row_takes_its_project_from_magpie_not_the_folder() {
        let snap = snapshot(
            vec![starling(
                "s1",
                Some(4242),
                "/elsewhere",
                SessionState::Working,
            )],
            &[(5151, 4242)],
        );
        let rows = compose(&snap, &[magpie("m1", 5151, Some(8))], &lookup);
        assert_eq!(rows[0].project_id, 8);
    }

    #[test]
    fn a_pid_that_is_not_the_parent_does_not_match() {
        // 5151's parent is 1 (launchd), not the claude process 4242.
        let snap = snapshot(
            vec![starling("s1", Some(4242), "/shop", SessionState::Waiting)],
            &[(5151, 1)],
        );
        let rows = compose(&snap, &[magpie("m1", 5151, Some(7))], &lookup);

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].magpie_session, None,
            "shown as starling-only instead"
        );
    }

    #[test]
    fn a_starling_session_with_no_pid_is_never_matched() {
        let snap = snapshot(
            vec![starling("s1", None, "/shop", SessionState::Waiting)],
            &[(5151, 4242)],
        );
        let rows = compose(&snap, &[magpie("m1", 5151, Some(7))], &lookup);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].magpie_session, None);
    }

    #[test]
    fn an_unmatched_live_session_resolves_its_folder_to_a_project() {
        let snap = snapshot(
            vec![
                starling("a", Some(1), "/shop", SessionState::Waiting),
                starling("b", Some(2), "/api", SessionState::Working),
            ],
            &[],
        );
        let rows = compose(&snap, &[], &lookup);
        let by_project: Vec<_> = rows
            .iter()
            .map(|r| (r.project_id, r.magpie_session.is_none()))
            .collect();
        assert_eq!(by_project, [(7, true), (8, true)]);
    }

    #[test]
    fn sessions_in_folders_magpie_does_not_know_are_left_out() {
        let snap = snapshot(
            vec![starling("a", Some(1), "/elsewhere", SessionState::Waiting)],
            &[],
        );
        assert!(compose(&snap, &[], &lookup).is_empty());
        let snap = snapshot(
            vec![starling(
                "a",
                Some(1),
                "/never-resolved",
                SessionState::Waiting,
            )],
            &[],
        );
        assert!(compose(&snap, &[], &lookup).is_empty());
    }

    #[test]
    fn idle_and_ended_sessions_are_not_agents_working_on_the_project() {
        let snap = snapshot(
            vec![
                starling("a", Some(1), "/shop", SessionState::Idle),
                starling("b", Some(2), "/shop", SessionState::Ended),
                starling("c", Some(3), "/shop", SessionState::Unknown),
            ],
            &[],
        );
        assert!(compose(&snap, &[], &lookup).is_empty());
    }

    #[test]
    fn waiting_sorts_before_working() {
        let snap = snapshot(
            vec![
                starling("w", Some(1), "/shop", SessionState::Working),
                starling("a", Some(2), "/shop", SessionState::Active),
                starling("z", Some(3), "/shop", SessionState::Waiting),
            ],
            &[],
        );
        let order: Vec<_> = compose(&snap, &[], &lookup)
            .iter()
            .map(|r| r.starling_session.clone())
            .collect();
        assert_eq!(order, ["z", "w", "a"]);
    }

    fn overview(project_id: Option<i64>, live: i64) -> ProjectOverview {
        ProjectOverview {
            project_id,
            project_name: "p".to_string(),
            now_count: 0,
            leased_count: 0,
            needs_review_count: 2,
            active_session_count: live,
            unfiled_count: None,
            unpromoted_count: 0,
            branch: None,
            now_cap: 7,
        }
    }

    #[test]
    fn across_counts_starling_only_sessions_as_live_and_every_waiting_agent_as_waiting() {
        let snap = snapshot(
            vec![
                // Matched: already one of magpie's live sessions, so not counted again as live.
                starling("m", Some(4242), "/shop", SessionState::Waiting),
                // Starling-only, waiting.
                starling("u1", Some(1), "/shop", SessionState::Waiting),
                // Starling-only, working.
                starling("u2", Some(2), "/shop", SessionState::Working),
            ],
            &[(5151, 4242)],
        );
        let agents = compose(&snap, &[magpie("m1", 5151, Some(7))], &lookup);

        let views = augment_overview(
            vec![
                overview(None, 0),
                overview(Some(7), 1),
                overview(Some(8), 0),
            ],
            &agents,
        );

        assert_eq!(views[0].agents_waiting_count, 0, "Inbox has no agents");
        assert_eq!(
            views[1].overview.active_session_count, 3,
            "1 magpie session + 2 starling-only"
        );
        assert_eq!(views[1].agents_waiting_count, 2);
        assert_eq!(
            views[1].overview.needs_review_count, 2,
            "handbacks stay what they were"
        );
        assert_eq!(views[2].overview.active_session_count, 0);
        assert_eq!(views[2].agents_waiting_count, 0);
    }

    #[test]
    fn the_overview_keeps_its_core_shape_when_serialized() {
        let views = augment_overview(vec![overview(Some(7), 1)], &[]);
        let json = serde_json::to_value(&views[0]).unwrap();
        assert_eq!(json["project_id"], 7);
        assert_eq!(json["active_session_count"], 1);
        assert_eq!(json["agents_waiting_count"], 0);
    }

    #[test]
    fn a_stream_pins_a_branch_only_when_it_is_not_the_default() {
        assert_eq!(
            branch_constraint(Some("refunds"), Some("main")).as_deref(),
            Some("refunds")
        );
        assert_eq!(branch_constraint(Some("main"), Some("main")), None);
        assert_eq!(branch_constraint(Some("trunk"), Some("trunk")), None);
        assert_eq!(
            branch_constraint(Some("main"), Some("trunk")).as_deref(),
            Some("main")
        );
        // git could not name a default: main and master stand in for it.
        assert_eq!(branch_constraint(Some("main"), None), None);
        assert_eq!(branch_constraint(Some("master"), None), None);
        assert_eq!(
            branch_constraint(Some("refunds"), None).as_deref(),
            Some("refunds")
        );
        // Detached or unknown branch: nothing to pin to.
        assert_eq!(branch_constraint(None, Some("main")), None);
        assert_eq!(branch_constraint(Some(""), Some("main")), None);
    }

    #[test]
    fn streams_in_known_projects_become_destinations() {
        let mut snap = snapshot(Vec::new(), &[]);
        snap.places.get_mut("/shop").unwrap().branch = Some("refunds".to_string());
        snap.places.get_mut("/api").unwrap().branch = Some("main".to_string());
        snap.streams = ["/shop", "/api", "/elsewhere", "/never-resolved"]
            .iter()
            .map(|cwd| StreamEntry {
                name: format!("stream{cwd}"),
                cwd: cwd.to_string(),
                default_branch: Some("main".to_string()),
            })
            .collect();

        let got = stream_destinations(&snap, &lookup);

        let shown: Vec<_> = got
            .iter()
            .map(|d| (d.project_id, d.stream.as_str(), d.branch.as_deref()))
            .collect();
        assert_eq!(
            shown,
            [(7, "stream/shop", Some("refunds")), (8, "stream/api", None)]
        );
        assert_eq!(got[0].project_name, "shop");
    }

    #[test]
    fn ps_output_is_read_as_pid_and_parent() {
        let parsed = parse_ps_parents("  5151  4242\n 9 1\nPID PPID\nnoise\n 7 8 9\n");
        assert_eq!(parsed, HashMap::from([(5151, 4242), (9, 1)]));
        assert!(parse_ps_parents("").is_empty());
    }
}
