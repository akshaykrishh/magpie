// Mirrors crates/magpie-core/src/model.rs and crates/magpie-capture/src/backend.rs
// field-for-field, since serde serializes struct fields as-is (snake_case,
// no rename). Keeping the wire shape identical here avoids a translation
// layer that could quietly drift from the Rust side.

export interface Capture {
  id: number;
  kind: string;
  body: string;
  created_at: string;
  done_at: string | null;
  failed_reason: string | null;
  queue_pos: number | null;
  project_id: number | null;
  branch: string | null;
  lease_session: string | null;
  lease_client: string | null;
  lease_pid: number | null;
  lease_at: string | null;
  lease_head_commit: string | null;
  handback_note: string | null;
  diff_stat: string | null;
  handback_at: string | null;
  source_id: number | null;
  merged_into: number | null;
  section_id: number | null;
  deleted_at: string | null;
  session_id: string | null;
  /// `"certain" | "guessed" | null` -- see migrations/0010_ui_provenance.sql.
  /// Left as `string | null` rather than a union: this crosses IPC as
  /// whatever Rust sends, and a third value appearing there should render
  /// as unknown (no chip) rather than fail a type check.
  filed_confidence: string | null;
}

export interface Project {
  id: number;
  name: string;
  remote_url: string | null;
  common_git_dir: string | null;
  created_at: string;
  last_active_at: string | null;
}

export interface Session {
  id: string;
  client: string | null;
  pid: number;
  project_id: number | null;
  branch: string | null;
  started_at: string;
  last_active_at: string | null;
  ended_at: string | null;
  leased_count: number;
  completed_count: number;
  failed_count: number;
  handback_count: number;
  captures_during_session: number | null;
  unpromoted_at_end: number | null;
  /// This session's small, stable, per-project label (S1, S2, ...) -- see
  /// migrations/0010_ui_provenance.sql. `null` only for rows written
  /// before that migration existed. S0 is never a value here; it's
  /// synthesized for the human, not a real Session row (see the redesign
  /// plan's stage 7).
  ordinal: number | null;
}

export interface SourceAppCount {
  app_name: string;
  count: number;
}

// One card in the session strip -- either a real Session (an agent
// connected over MCP) or the synthesized S0 card for the human's own use
// of the app. See src-tauri/src/sessions_view.rs's doc comment for why
// this is one shape for both rather than a union: fields that don't apply
// to a given kind are simply left at their zero value.
export interface SessionView {
  /// `0` only for the synthesized S0 card -- never a real session's ordinal.
  ordinal: number;
  is_synthetic: boolean;
  client: string | null;
  branch: string | null;
  started_at: string;
  holds: number;
  drained: number;
  failed_count: number;
  handback_count: number;
  /// `null` for a real session; `number` (captures since app launch) for S0.
  captures_since: number | null;
  /// Non-empty only for S0.
  top_source_apps: SourceAppCount[];
}

// Not a raw table row -- a computed rollup (see
// crates/magpie-core/src/projects.rs's list_projects_overview). project_id
// is null for the pinned "Inbox" entry, which is always present even with
// zero projects.
export interface ProjectOverview {
  project_id: number | null;
  project_name: string;
  now_count: number;
  leased_count: number;
  needs_review_count: number;
  active_session_count: number;
  /// Total unfiled captures -- only non-null on the Inbox row (`project_id:
  /// null`). Backs Across's `Unfiled` row.
  unfiled_count: number | null;
  /// This scope's stream captures not yet promoted to Now.
  unpromoted_count: number;
  /// The most recently active session's branch (live preferred over
  /// ended) -- `null` when unknown, or on the Inbox row, where branch
  /// never applies. See the redesign plan's "render as unknown, never
  /// invent": this is the only honest source, the desktop app has no git
  /// access of its own.
  branch: string | null;
  /// Display-only Now capacity (see `settings.now_cap`) -- same value on
  /// every row. Never enforced.
  now_cap: number;
}

export interface Tag {
  id: number;
  name: string;
}

export interface Section {
  id: number;
  name: string;
  position: number;
  created_at: string;
  deleted_at: string | null;
}

export interface Template {
  id: number;
  title: string;
  body: string;
  created_at: string;
  description: string | null;
  variables_json: string | null;
  pack_id: number | null;
  section_id: number | null;
  deleted_at: string | null;
}

export interface AuditEntry {
  id: number;
  at: string;
  actor: string;
  action: string;
  capture_id: number | null;
  session_id: string | null;
}

// A computed rollup, not a raw audit row -- see
// crates/magpie-core/src/audit.rs's `list_audit_enriched`. Every
// `session_*` field is `null` for rows written before migration 0010
// added `audit.session_id`; the Activity overlay groups those into an
// explicit "Earlier" bucket rather than guessing.
export interface AuditEntryView {
  id: number;
  at: string;
  actor: string;
  action: string;
  capture_id: number | null;
  session_id: string | null;
  session_ordinal: number | null;
  session_client: string | null;
  session_branch: string | null;
  session_live: boolean | null;
}

// One stream row, joined -- see crates/magpie-core/src/stream.rs's
// `StreamRow`. Nests `capture` rather than flattening its fields, mirroring
// the Rust shape exactly.
export interface StreamRow {
  capture: Capture;
  source_app_name: string | null;
  source_window_title: string | null;
  source_url: string | null;
  /// Non-null only when this capture has a screenshot blob -- check this,
  /// not `ocr_text`, to know whether to render a thumbnail at all (see
  /// the Rust struct's doc comment on why `ocr_text` alone is ambiguous).
  blob_mime: string | null;
  blob_width: number | null;
  blob_height: number | null;
  ocr_text: string | null;
  project_name: string | null;
  merged_count: number;
  session_ordinal: number | null;
  session_client: string | null;
}

export interface Blob {
  id: number;
  capture_id: number;
  path: string;
  mime: string;
  width: number | null;
  height: number | null;
  ocr_text: string | null;
}

export type CaptureMode = "clipboard_only" | "synthesized_copy";

export interface Capabilities {
  mode: CaptureMode;
  synthesized_copy_available: boolean;
  screenshot_available: boolean;
  ocr_available: boolean;
}

// Wire type for magpie-core's list_stream project filter -- a tagged enum
// on the Rust side rather than a nullable-nullable value, since serde's
// default Option<Option<T>> handling collapses "don't filter" and "filter
// to no project" to the same JSON null.
export type ProjectFilter =
  | { kind: "all" }
  | { kind: "inbox" }
  | { kind: "project"; id: number };

// Mirrors the JSON object returned by `get_hotkey_settings`.
export interface HotkeySettings {
  capture: string;
  screenshot: string;
}

// Mirrors `SETTING_KEYS` in src-tauri/src/settings_commands.rs -- the
// Rust command rejects any other key, so keep this list in sync with that
// one rather than typing `key: string` and letting a typo surface only at
// runtime.
export type SettingKey =
  | "theme"
  | "now_cap"
  | "quiet_until"
  | "toast_capture_count"
  | "clipboard_only_capture_count"
  | "permission_offer_dismissed"
  | "onboarding_complete"
  | "pinned_project_id"
  | "update_auto_check"
  | "update_skipped_version";

// Mirrors updater.rs's `UpdateStatus` (`#[serde(tag = "kind", rename_all
// = "snake_case")]`) -- every field name is the Rust struct's own field
// name verbatim, since serde's `rename_all` only renames variant tags,
// not field names.
export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "up_to_date"; checked_at: string }
  | {
      kind: "available";
      version: string;
      notes: string | null;
      pub_date: string | null;
    }
  | { kind: "downloading"; downloaded: number; total: number | null }
  | { kind: "ready"; version: string; notes: string | null }
  | { kind: "skipped"; version: string }
  | { kind: "unsupported"; reason: string }
  | { kind: "failed"; message: string; checked_at: string };
