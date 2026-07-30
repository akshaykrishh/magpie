// Mirrors crates/magpie-core/src/model.rs and crates/magpie-capture/src/backend.rs
// field-for-field, since serde serializes struct fields as-is (snake_case,
// no rename). Keeping the wire shape identical here avoids a translation
// layer that could quietly drift from the Rust side.

export interface Capture {
  id: number;
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
  source_id: number | null;
  merged_into: number | null;
}

export interface Project {
  id: number;
  name: string;
  remote_url: string | null;
  common_git_dir: string | null;
  created_at: string;
}

export interface Tag {
  id: number;
  name: string;
}

export interface Template {
  id: number;
  title: string;
  body: string;
  created_at: string;
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
