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
}

export interface AuditEntry {
  id: number;
  at: string;
  actor: string;
  action: string;
  capture_id: number | null;
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
