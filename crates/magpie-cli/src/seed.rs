//! `magpie seed --tier <t>` -- writes a fixture database at one of five
//! data-density tiers, so the redesign's "fluid across data density"
//! principle (see the redesign plan) is verifiable against the real app,
//! not just eyeballed in the gallery. Companion to
//! apps/desktop/src/GalleryApp.tsx, which renders the same tiers from
//! hand-built fixtures for surfaces that don't have a live window yet.
//!
//! Every fixture is built through real `Store` methods (capture, promote,
//! queue_take, capture_handback, ...), never raw SQL -- so seeding a tier
//! also exercises the exact code paths (FTS triggers, recency touches,
//! session counters) a real client would, rather than producing rows that
//! merely look right.
//!
//! Never touches the real database unless explicitly told to: see
//! `resolve_db_path`.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::ValueEnum;
use magpie_core::{LeaseIdentity, NewSource, Store};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Tier {
    /// Fresh install, nothing -- exercises every "no signal yet" / earned-
    /// surface-absent state.
    Zero,
    /// ~13 captures, 1 project, 0 sessions -- mirrors the real desk this
    /// redesign was planned against (see the redesign plan's "sparse tier
    /// reference").
    Sparse,
    /// 1 project, Now populated, 1 live session with a lease in flight.
    Working,
    /// 4 projects, 3 sessions (including a branch collision), 1,284
    /// captures -- the tier the design doc itself is drawn at.
    Dense,
    /// One axis pathological at a time, several axes at once: hundreds of
    /// unfiled captures, Now over the display cap, a session that
    /// connected but never called a tool (client stays NULL), every blob
    /// stuck OCR-pending, and one capture with a 40,000-character body.
    Degenerate,
}

const TEXT_CAPTURES: &[&str] = &[
    "Add strict unknown-key validation with helpful suggestions",
    "Resolve paths relative to the configuration file",
    "A command that prints the fully resolved configuration",
    "Explicit environment access rather than interpolation",
    "Clear precedence: flags -> env -> config -> defaults",
    "Negation in inherited configs -- the moment a config can extend a base \
     or preset, someone needs to remove an extension the base added",
    "keep the core configuration declarative even if you later add an \
     optional TypeScript escape hatch",
    "Three things worth locking down before it ships",
    "Cap OCR worker memory -- 400MB on a 4k region shot",
];

/// Resolves where a seed run writes to. Never defaults to the real
/// `default_db_path()` -- the whole point of this command is producing
/// fixtures without risking the user's actual data. `--db` is required to
/// point anywhere else, and doing so over an existing file requires
/// `--force`, so pointing it (even by mistake) at a real database refuses
/// rather than silently overwriting it.
pub fn resolve_db_path(tier: Tier, db: Option<PathBuf>, force: bool) -> Result<PathBuf> {
    match db {
        None => {
            let dir = dirs::data_dir()
                .context("could not determine a data directory for this platform")?
                .join("magpie");
            std::fs::create_dir_all(&dir)?;
            Ok(dir.join(format!("seed-{}.db", tier_name(tier))))
        }
        Some(path) => {
            if path.exists() && !force {
                bail!(
                    "{path:?} already exists -- pass --force to overwrite it (double check this \
                     isn't your real magpie.db: `magpie` with no --db always uses the OS default \
                     data directory, never this flag)"
                );
            }
            Ok(path)
        }
    }
}

fn tier_name(tier: Tier) -> &'static str {
    match tier {
        Tier::Zero => "zero",
        Tier::Sparse => "sparse",
        Tier::Working => "working",
        Tier::Dense => "dense",
        Tier::Degenerate => "degenerate",
    }
}

pub fn run(tier: Tier, db_path: &PathBuf) -> Result<()> {
    if db_path.exists() {
        std::fs::remove_file(db_path).with_context(|| format!("removing stale {db_path:?}"))?;
        let wal = db_path.with_extension("db-wal");
        let shm = db_path.with_extension("db-shm");
        let _ = std::fs::remove_file(wal);
        let _ = std::fs::remove_file(shm);
    }
    let store = Store::open(db_path).with_context(|| format!("opening {db_path:?}"))?;

    match tier {
        Tier::Zero => seed_zero(&store)?,
        Tier::Sparse => seed_sparse(&store)?,
        Tier::Working => seed_working(&store)?,
        Tier::Dense => seed_dense(&store)?,
        Tier::Degenerate => seed_degenerate(&store)?,
    }

    Ok(())
}

/// Nothing at all -- migrations already ran via `Store::open`.
fn seed_zero(_store: &Store) -> Result<()> {
    Ok(())
}

/// Mirrors the real desk this redesign was planned against: 13 captures,
/// 10 unfiled, 1 in Now (in Inbox, not filed to the project), 1 project,
/// 0 sessions, 4 templates, 1 blob with OCR already finished.
fn seed_sparse(store: &Store) -> Result<()> {
    let project = store.get_or_create_project(
        "magpie",
        Some("https://github.com/akshaykrishh/magpie.git"),
        None,
    )?;

    // 3 filed to the project.
    for body in TEXT_CAPTURES.iter().take(3) {
        let c = store.capture(body, None)?;
        store.assign_project(c.id, Some(project.id))?;
    }

    // 8 unfiled, plain text -- plus the screenshot and the Now item below,
    // that's 10 unfiled total against 3 filed = 13 captures, matching the
    // redesign plan's "sparse tier reference". Indexed and cycled through
    // TEXT_CAPTURES rather than sliced past its end (that undercounted
    // silently on the first pass at this fixture -- `.skip(3).take(9)`
    // against a 9-element array only yields 6, not 9).
    for i in 0..8 {
        store.capture(TEXT_CAPTURES[i % TEXT_CAPTURES.len()], None)?;
    }

    // 1 unfiled screenshot, OCR already done -- this is the 10th unfiled
    // capture and the app's one blob.
    let shot = store.capture_screenshot(
        "/dev/null/fixture.png",
        "image/png",
        Some(1600),
        Some(900),
        Some(NewSource {
            app_name: Some("Terminal".to_string()),
            ..Default::default()
        }),
    )?;
    if let Some(blob) = store.get_blob_for_capture(shot.id)? {
        store.set_blob_ocr_text(blob.id, "tool.toml -- a good convention would be...")?;
    }

    // The one Now item -- promoted while still unfiled (Inbox's own Now,
    // per the redesign plan's "there is no default focused project"
    // reasoning: this is exactly why that item stays visible rather than
    // being scoped away by a project the user never chose).
    let now_item = store.capture("Write the ROADMAP.md Now/Next/Later pass", None)?;
    store.promote(now_item.id)?;

    for title in ["Bug report", "Design review", "Release notes", "Postmortem"] {
        store.create_template(title, &format!("## {title}\n\n"))?;
    }

    Ok(())
}

/// 1 project, Now populated, 1 live session mid-lease.
fn seed_working(store: &Store) -> Result<()> {
    let project = store.get_or_create_project(
        "magpie-core",
        Some("https://github.com/x/magpie-core.git"),
        None,
    )?;

    for body in TEXT_CAPTURES.iter().take(5) {
        let c = store.capture(body, None)?;
        store.assign_project(c.id, Some(project.id))?;
        store.promote(c.id)?;
    }
    for body in TEXT_CAPTURES.iter().skip(5) {
        store.capture(body, None)?;
    }

    let session_id = uuid_like("s1");
    store.create_session(
        &session_id,
        5001,
        Some(project.id),
        Some("feat/config-schema"),
    )?;
    store.touch_session_active(&session_id, "claude-code")?;

    let identity = LeaseIdentity {
        session: session_id.clone(),
        client: "claude-code".to_string(),
        pid: 5001,
    };
    store.queue_take(Some(project.id), Some("feat/config-schema"), &identity)?;

    for title in ["Bug report", "Design review"] {
        store.create_template(title, &format!("## {title}\n\n"))?;
    }

    Ok(())
}

/// 4 projects, 3 sessions (2 live -- including a same-project, different-
/// branch collision -- 1 ended with a digest), 1,284 captures spread
/// across all of them plus Inbox, with a lease, a hand-back, and a
/// failure in flight so every capture-level state has at least one row.
fn seed_dense(store: &Store) -> Result<()> {
    let magpie_core = store.get_or_create_project(
        "magpie-core",
        Some("https://github.com/x/magpie-core.git"),
        None,
    )?;
    let rook_api =
        store.get_or_create_project("rook-api", Some("https://github.com/x/rook-api.git"), None)?;
    let telemetry_lab = store.get_or_create_project(
        "telemetry-lab",
        Some("https://github.com/x/telemetry-lab.git"),
        None,
    )?;
    let saakshi =
        store.get_or_create_project("saakshi", Some("https://github.com/x/saakshi.git"), None)?;
    let projects = [magpie_core.id, rook_api.id, telemetry_lab.id, saakshi.id];

    const TOTAL: usize = 1284;
    let mut promoted_in_magpie_core = Vec::new();
    for i in 0..TOTAL {
        let phrase = TEXT_CAPTURES[i % TEXT_CAPTURES.len()];
        let c = store.capture(&format!("{phrase} (#{i})"), None)?;
        // Roughly 60% filed, rest stay in Inbox -- a dense project still
        // has an Unfiled tail, it's just a small fraction of the whole.
        if i % 5 != 0 {
            let project_id = projects[i % projects.len()];
            store.assign_project(c.id, Some(project_id))?;
            if project_id == magpie_core.id && promoted_in_magpie_core.len() < 6 {
                store.promote(c.id)?;
                promoted_in_magpie_core.push(c.id);
            }
        }
    }

    // S1: live, on magpie-core's feat/config-schema, mid-lease.
    let s1 = uuid_like("s1");
    store.create_session(&s1, 6001, Some(magpie_core.id), Some("feat/config-schema"))?;
    store.touch_session_active(&s1, "claude-code")?;
    let s1_identity = LeaseIdentity {
        session: s1.clone(),
        client: "claude-code".to_string(),
        pid: 6001,
    };
    if let Some(leased) = store.queue_take(
        Some(magpie_core.id),
        Some("feat/config-schema"),
        &s1_identity,
    )? {
        // One hand-back, so the review sheet (stage 6) has a real row to
        // render against -- diff_stat mirrors a real `git diff --stat`.
        store.capture_handback(
            leased.id,
            &s1,
            "Ready for review -- renamed the validator but didn't touch the schema loader",
            Some(" src/config/schema.rs | 64 ++++++++++++++++++++++----\n src/config/mod.rs    | 11 -----\n 2 files changed, 64 insertions(+), 11 deletions(-)"),
        )?;
    }
    if let Some(second) = store.queue_take(
        Some(magpie_core.id),
        Some("feat/config-schema"),
        &s1_identity,
    )? {
        store.capture_fail(
            second.id,
            &s1,
            "couldn't find src/config/schema.rs on this branch",
        )?;
    }

    // S2: live, SAME project, DIFFERENT branch -- the branch collision
    // this tier exists to exercise (off-branch dimming, once stage 4's
    // `pin_to_branch` and stage 6's row rendering land).
    let s2 = uuid_like("s2");
    store.create_session(&s2, 6002, Some(magpie_core.id), Some("fix/oom-on-ocr"))?;
    store.touch_session_active(&s2, "cursor")?;

    // S3: ended, with a digest (end_session writes one as a capture).
    let s3 = uuid_like("s3");
    store.create_session(&s3, 6003, Some(rook_api.id), Some("main"))?;
    store.touch_session_active(&s3, "claude-code")?;
    let s3_identity = LeaseIdentity {
        session: s3.clone(),
        client: "claude-code".to_string(),
        pid: 6003,
    };
    if let Some(item) = store.queue_take(Some(rook_api.id), Some("main"), &s3_identity)? {
        store.capture_complete(item.id, &s3)?;
    }
    store.end_session(&s3)?;

    for title in ["Bug report", "Design review", "Release notes", "Postmortem"] {
        let t = store.create_template(title, &format!("## {title}\n\n"))?;
        store.assign_template_section(t.id, None)?;
    }

    Ok(())
}

/// A grab-bag stress fixture: every degenerate axis the redesign plan
/// calls out, seeded at once rather than one at a time, so a single
/// `magpie seed --tier degenerate` run exercises all of them together.
fn seed_degenerate(store: &Store) -> Result<()> {
    // 400 unfiled captures, and nothing else filed anywhere -- this
    // project exists (so its name is visible) but nothing ties a capture
    // to it, and no session ever references it either, so its branch is
    // genuinely unknown by construction, not just unset.
    let _unreferenced_project = store.get_or_create_project(
        "magpie-core",
        Some("https://github.com/x/magpie-core.git"),
        None,
    )?;

    let mut unfiled_ids = Vec::new();
    for i in 0..400 {
        let phrase = TEXT_CAPTURES[i % TEXT_CAPTURES.len()];
        let c = store.capture(&format!("{phrase} (#{i})"), None)?;
        unfiled_ids.push(c.id);
    }

    // Now over the display cap: 10 promoted into Inbox's own Now (still
    // unfiled -- see seed_sparse's comment on why Inbox has its own Now),
    // against a cap that's always been display-only (settings.now_cap).
    for id in unfiled_ids.iter().take(10) {
        store.promote(*id)?;
    }

    // A session that connected but never called a tool: client stays NULL
    // forever (touch_session_active is deliberately never called).
    let ghost_session = uuid_like("ghost");
    store.create_session(&ghost_session, 7001, None, None)?;

    // Every blob stuck OCR-pending: 5 screenshots, ocr_text left NULL.
    for i in 0..5 {
        store.capture_screenshot(
            &format!("/dev/null/fixture-{i}.png"),
            "image/png",
            Some(1600),
            Some(900),
            None,
        )?;
    }

    // One capture with a 40,000-character body. Computed from the actual
    // phrase length and truncated by char count (not byte slicing) so this
    // can't panic on an off-by-few miscount or a future phrase that isn't
    // pure ASCII.
    const TARGET_LEN: usize = 40_000;
    let phrase = "This line is part of an unreasonably long capture body. ";
    let repeats = TARGET_LEN / phrase.chars().count() + 1;
    let giant_body: String = phrase.repeat(repeats).chars().take(TARGET_LEN).collect();
    store.capture(&giant_body, None)?;

    Ok(())
}

/// Deterministic, readable session ids for fixtures (`s1`, `s2`, ...)
/// rather than real UUIDs -- production code always uses UUID v4 (see
/// magpie-mcp's `MagpieServer::new`), but a fixture is easier to eyeball
/// and grep for in `sqlite3` when its id is legible.
fn uuid_like(label: &str) -> String {
    format!("seed-{label}-0000-0000-000000000000")
}
