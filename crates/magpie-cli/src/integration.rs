//! The read-only JSON surface other tools use to see magpie's state: `magpie
//! now --json` and `magpie sessions --json`. Output is versioned by a
//! top-level `schema` field; see docs/integration.md for the contract.
//!
//! Everything here only reads. Callers open the store with
//! `Store::open_read_only`, and `None` (no database yet) is a normal answer:
//! an empty report, never an error and never a freshly created database.

use std::collections::HashMap;

use anyhow::Result;
use magpie_core::{Capture, Store};
use magpie_mcp::DetectedProject;
use serde_json::{json, Value};

const SCHEMA: u32 = 1;

/// Which project's Now to report.
pub enum Scope {
    /// No `--project`: the Inbox scope, the same one `list --now` reads.
    Inbox,
    /// `--project <dir>`: the identity that folder resolves to, as the MCP
    /// server would resolve it from its spawn directory.
    Folder(DetectedProject),
}

/// Opens the database for a JSON read. A missing database and one older than
/// this build both answer with the empty report (`None`), because other
/// tools read this and the contract has no error shape; the old one also
/// gets a hint on stderr, since only launching magpie can upgrade it.
pub fn open_for_report(path: &std::path::Path) -> Result<Option<Store>> {
    match Store::open_read_only(path) {
        Err(magpie_core::Error::SchemaOutdated) => {
            eprintln!("magpie: {}", magpie_core::Error::SchemaOutdated);
            Ok(None)
        }
        other => Ok(other?),
    }
}

/// The first line of an item, untruncated -- the item's title.
fn title(body: &str) -> &str {
    body.lines().next().unwrap_or("").trim_end()
}

fn project_json(id: i64, name: &str) -> Value {
    json!({ "id": id, "name": name })
}

/// Finds the folder's project without creating it, matching on the same
/// identity the MCP server files under (see `get_or_create_project`).
fn resolve_folder(store: &Store, detected: &DetectedProject) -> Result<Option<(i64, String)>> {
    if detected.remote_url.is_none() && detected.common_git_dir.is_none() {
        return Ok(None);
    }
    let project = store.find_project(
        detected.remote_url.as_deref(),
        detected.common_git_dir.as_deref(),
    )?;
    Ok(project.map(|p| (p.id, p.name)))
}

fn lease_json(c: &Capture) -> Value {
    match &c.lease_session {
        Some(session) => json!({ "session": session, "pid": c.lease_pid, "at": c.lease_at }),
        None => Value::Null,
    }
}

/// `magpie now --json`. `position` is the item's 1-based rank in the
/// project's whole Now queue, so it does not shift with `--branch`.
pub fn now_report(store: Option<&Store>, scope: &Scope, branch: Option<&str>) -> Result<Value> {
    let empty = json!({ "schema": SCHEMA, "project": null, "items": [] });
    let Some(store) = store else {
        return Ok(empty);
    };

    let (project, queue) = match scope {
        Scope::Folder(detected) => match resolve_folder(store, detected)? {
            Some((id, name)) => (project_json(id, &name), store.list_now(Some(id))?),
            None => return Ok(empty),
        },
        Scope::Inbox => (Value::Null, store.list_now(None)?),
    };

    let items: Vec<Value> = queue
        .iter()
        .enumerate()
        .filter(|(_, c)| match (&c.branch, branch) {
            (Some(constraint), Some(wanted)) => constraint == wanted,
            _ => true,
        })
        .map(|(i, c)| {
            json!({
                "id": c.id,
                "title": title(&c.body),
                "branch": c.branch,
                "position": i + 1,
                "lease": lease_json(c),
            })
        })
        .collect();

    Ok(json!({ "schema": SCHEMA, "project": project, "items": items }))
}

/// `magpie sessions --json`, newest first.
pub fn sessions_report(store: Option<&Store>, live_only: bool) -> Result<Value> {
    let Some(store) = store else {
        return Ok(json!({ "schema": SCHEMA, "sessions": [] }));
    };

    let names: HashMap<i64, String> = store
        .list_projects()?
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect();

    let mut sessions = Vec::new();
    for s in store.list_sessions(None)? {
        if live_only && s.ended_at.is_some() {
            continue;
        }
        let project = s
            .project_id
            .and_then(|id| names.get(&id).map(|name| project_json(id, name)))
            .unwrap_or(Value::Null);
        let leased = match store.leased_capture_for_session(&s.id)? {
            Some(c) => json!({ "id": c.id, "title": title(&c.body) }),
            None => Value::Null,
        };
        sessions.push(json!({
            "id": s.id,
            "client": s.client,
            "pid": s.pid,
            "project": project,
            "branch": s.branch,
            "started_at": s.started_at,
            "ended_at": s.ended_at,
            "leased": leased,
        }));
    }

    Ok(json!({ "schema": SCHEMA, "sessions": sessions }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use magpie_core::LeaseIdentity;

    const REMOTE: &str = "git@github.com:acme/shop-api.git";

    fn detected(remote: &str) -> DetectedProject {
        DetectedProject {
            remote_url: Some(remote.to_string()),
            ..Default::default()
        }
    }

    fn queued(store: &Store, project: Option<i64>, body: &str) -> Capture {
        let c = store.capture(body, None).unwrap();
        store.assign_project(c.id, project).unwrap();
        store.promote(c.id).unwrap()
    }

    fn store_with_project() -> (Store, i64) {
        let store = Store::open_in_memory().unwrap();
        let project = store
            .get_or_create_project("shop-api", Some(REMOTE), None)
            .unwrap();
        (store, project.id)
    }

    fn folder_now(store: &Store, remote: &str, branch: Option<&str>) -> Value {
        now_report(Some(store), &Scope::Folder(detected(remote)), branch).unwrap()
    }

    #[test]
    fn now_has_the_schema_1_shape() {
        let (store, pid) = store_with_project();
        queued(&store, Some(pid), "Retry webhook\nmore detail");

        let out = folder_now(&store, REMOTE, None);
        assert_eq!(out["schema"], 1);
        assert_eq!(out["project"], json!({ "id": pid, "name": "shop-api" }));
        let item = &out["items"][0];
        for key in ["id", "title", "branch", "position", "lease"] {
            assert!(item.get(key).is_some(), "missing {key}");
        }
        assert_eq!(item["position"], 1);
        assert!(item["branch"].is_null());
        assert!(item["lease"].is_null());
    }

    #[test]
    fn title_is_the_first_line() {
        let (store, pid) = store_with_project();
        queued(&store, Some(pid), "Fix the redirect\n\nlong body\nmore");
        let out = folder_now(&store, REMOTE, None);
        assert_eq!(out["items"][0]["title"], "Fix the redirect");
    }

    #[test]
    fn unknown_project_is_null_and_empty() {
        let (store, pid) = store_with_project();
        queued(&store, Some(pid), "something");

        let out = folder_now(&store, "git@github.com:acme/other.git", None);
        assert_eq!(out, json!({ "schema": 1, "project": null, "items": [] }));
        // A read never creates the project it could not find.
        assert_eq!(store.list_projects().unwrap().len(), 1);
    }

    #[test]
    fn folder_with_no_git_identity_is_null_and_empty() {
        let store = Store::open_in_memory().unwrap();
        let out = now_report(
            Some(&store),
            &Scope::Folder(DetectedProject::default()),
            None,
        )
        .unwrap();
        assert_eq!(out, json!({ "schema": 1, "project": null, "items": [] }));
    }

    #[test]
    fn missing_database_is_null_and_empty() {
        let out = now_report(None, &Scope::Inbox, None).unwrap();
        assert_eq!(out, json!({ "schema": 1, "project": null, "items": [] }));
        let out = sessions_report(None, true).unwrap();
        assert_eq!(out, json!({ "schema": 1, "sessions": [] }));
    }

    #[test]
    fn no_project_reads_the_inbox_like_list_now() {
        let (store, pid) = store_with_project();
        queued(&store, Some(pid), "in a project");
        queued(&store, None, "in the inbox");

        let out = now_report(Some(&store), &Scope::Inbox, None).unwrap();
        assert!(out["project"].is_null());
        assert_eq!(out["items"].as_array().unwrap().len(), 1);
        assert_eq!(out["items"][0]["title"], "in the inbox");
    }

    #[test]
    fn branch_filter_keeps_unconstrained_and_matching_items() {
        let (store, pid) = store_with_project();
        queued(&store, Some(pid), "anywhere");
        let on_refunds = queued(&store, Some(pid), "refunds only");
        store.pin_to_branch(on_refunds.id, Some("refunds")).unwrap();
        let on_other = queued(&store, Some(pid), "other only");
        store.pin_to_branch(on_other.id, Some("other")).unwrap();

        let out = folder_now(&store, REMOTE, Some("refunds"));
        let titles: Vec<_> = out["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles, ["anywhere", "refunds only"]);
        // Position is the rank in the whole queue, not in the filtered list.
        assert_eq!(out["items"][1]["position"], 2);

        let all = folder_now(&store, REMOTE, None);
        assert_eq!(all["items"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn leased_item_reports_its_lease_and_session_reports_the_item() {
        let (store, pid) = store_with_project();
        let item = queued(&store, Some(pid), "Retry webhook\ndetail");
        store.create_session("s-1", 5151, Some(pid), None).unwrap();
        let identity = LeaseIdentity {
            session: "s-1".into(),
            client: "claude-code".into(),
            pid: 5151,
        };
        store.queue_take(Some(pid), None, &identity).unwrap();

        let now = folder_now(&store, REMOTE, None);
        let lease = &now["items"][0]["lease"];
        assert_eq!(lease["session"], "s-1");
        assert_eq!(lease["pid"], 5151);
        assert!(lease["at"].is_string());

        let out = sessions_report(Some(&store), false).unwrap();
        assert_eq!(
            out["sessions"][0]["leased"],
            json!({ "id": item.id, "title": "Retry webhook" })
        );
    }

    #[test]
    fn sessions_have_the_schema_1_shape() {
        let (store, pid) = store_with_project();
        store
            .create_session("s-1", 5151, Some(pid), Some("refunds"))
            .unwrap();
        store.create_session("s-2", 6000, None, None).unwrap();

        let out = sessions_report(Some(&store), false).unwrap();
        assert_eq!(out["schema"], 1);
        let sessions = out["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 2);
        let s1 = sessions.iter().find(|s| s["id"] == "s-1").unwrap();
        for key in [
            "id",
            "client",
            "pid",
            "project",
            "branch",
            "started_at",
            "ended_at",
            "leased",
        ] {
            assert!(s1.get(key).is_some(), "missing {key}");
        }
        assert_eq!(s1["project"], json!({ "id": pid, "name": "shop-api" }));
        assert_eq!(s1["branch"], "refunds");
        assert!(s1["client"].is_null());
        assert!(s1["ended_at"].is_null());
        assert!(s1["leased"].is_null());
        let s2 = sessions.iter().find(|s| s["id"] == "s-2").unwrap();
        assert!(s2["project"].is_null());
    }

    #[test]
    fn live_excludes_ended_sessions() {
        let store = Store::open_in_memory().unwrap();
        store.create_session("live", 1, None, None).unwrap();
        store.create_session("gone", 2, None, None).unwrap();
        store.end_session("gone").unwrap();

        let all = sessions_report(Some(&store), false).unwrap();
        assert_eq!(all["sessions"].as_array().unwrap().len(), 2);

        let live = sessions_report(Some(&store), true).unwrap();
        let sessions = live["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["id"], "live");
    }

    #[test]
    fn reads_never_write_to_an_existing_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("magpie.db");
        {
            let store = Store::open(&path).unwrap();
            store.create_session("s-1", 1, None, None).unwrap();
        }
        let before = std::fs::read(&path).unwrap();

        let store = Store::open_read_only(&path).unwrap().unwrap();
        sessions_report(Some(&store), true).unwrap();
        now_report(Some(&store), &Scope::Inbox, None).unwrap();
        assert!(store.create_session("s-2", 2, None, None).is_err());
        drop(store);

        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    /// Everything in `dir`, as (name, bytes), so a test can prove a read
    /// changed nothing and left nothing behind.
    fn snapshot(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read(e.path()).unwrap(),
                )
            })
            .collect();
        files.sort();
        files
    }

    #[test]
    fn a_zero_byte_file_is_left_alone_too() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("magpie.db");
        std::fs::write(&path, b"").unwrap();
        let before = snapshot(dir.path());

        assert!(matches!(
            Store::open_read_only(&path).err(),
            Some(magpie_core::Error::SchemaOutdated)
        ));
        assert_eq!(snapshot(dir.path()), before);
        assert!(
            open_for_report(&path).unwrap().is_none(),
            "the reports see it as empty"
        );
    }

    #[test]
    fn open_read_only_does_not_create_a_missing_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("magpie.db");
        assert!(Store::open_read_only(&path).unwrap().is_none());
        assert!(!path.exists());
        assert!(!path.parent().unwrap().exists());
    }
}
