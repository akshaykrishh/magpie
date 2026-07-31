# Frontend Foundation — Sync Types and API (Frontend Phase A) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `apps/desktop/src/lib/types.ts` and `apps/desktop/src/lib/api.ts` up to date with everything the backend now provides (five backend phases' worth of new fields and commands), so every later frontend phase can build against real types instead of stale ones.

**Architecture:** Pure additive changes to two files — no components, no behavior change. `types.ts` gains the fields/interfaces the Rust side already has (`Capture.kind`/`lease_head_commit`/`handback_note`/`diff_stat`/`handback_at`, `Project.last_active_at`, new `Session` and `ProjectOverview` interfaces). `api.ts` gains wrappers for the two Tauri commands that exist but have no frontend caller yet (`list_sessions`, `list_projects_overview`).

**Tech Stack:** TypeScript, no new dependencies.

## Global Constraints

- **`capture_handback` gets no API wrapper and no new Tauri command.** It's lease-gated (`require_lease_tx` in `crates/magpie-core/src/lease.rs`), and the desktop app never holds a lease — leases are established only via MCP's `queue_take`, which only an agent connection does. A desktop-callable `capture_handback` would always fail if actually invoked from the UI. The "handed-back" state is read-only from the desktop app's perspective (already fully present on `Capture` once `types.ts` is updated) — closing it out happens via the existing `mark_capture_done` command, unchanged.
- Field names and shapes must match the Rust structs exactly (snake_case, no translation layer) — this file's own header comment states that convention; follow it.
- No component changes in this plan — `CaptureItem.tsx`/`NowList.tsx`/`App.tsx` etc. are all out of scope; they don't consume any of these new fields yet (that's Frontend Phase B).

---

### Task 1: Sync `types.ts` and `api.ts`

**Files:**
- Modify: `apps/desktop/src/lib/types.ts`
- Modify: `apps/desktop/src/lib/api.ts`

**Interfaces:**
- Produces: `Capture.kind: string`, `Capture.lease_head_commit: string | null`, `Capture.handback_note: string | null`, `Capture.diff_stat: string | null`, `Capture.handback_at: string | null` — consumed by Frontend Phase B.
- Produces: `Project.last_active_at: string | null` — consumed by a future phase (project ordering/recency display).
- Produces: `Session` interface (13 fields, mirroring `crates/magpie-core/src/model.rs`) — consumed by Frontend Phase C (session strip).
- Produces: `ProjectOverview` interface (6 fields, mirroring `crates/magpie-core/src/projects.rs`) — consumed by a future "Across" phase.
- Produces: `api.listSessions(projectId: number | null)`, `api.listProjectsOverview()` — consumed by Frontend Phases C and later.

- [ ] **Step 1: Update the `Capture` and `Project` interfaces**

In `apps/desktop/src/lib/types.ts`, change:

```ts
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
```

to:

```ts
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
}

export interface Project {
  id: number;
  name: string;
  remote_url: string | null;
  common_git_dir: string | null;
  created_at: string;
  last_active_at: string | null;
}
```

- [ ] **Step 2: Add `Session` and `ProjectOverview` interfaces**

In `apps/desktop/src/lib/types.ts`, add after the `Project` interface:

```ts
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
```

- [ ] **Step 3: Add the two missing API wrappers**

In `apps/desktop/src/lib/api.ts`, change the import line:

```ts
import type {
  AuditEntry,
  Blob,
  Capabilities,
  Capture,
  Project,
  ProjectFilter,
  Tag,
  Template,
} from "./types";
```

to:

```ts
import type {
  AuditEntry,
  Blob,
  Capabilities,
  Capture,
  Project,
  ProjectFilter,
  ProjectOverview,
  Session,
  Tag,
  Template,
} from "./types";
```

Add, after `listProjects`/`getOrCreateProject` (before `exportJson`):

```ts
  listSessions: (projectId: number | null) =>
    invoke<Session[]>("list_sessions", { projectId }),

  listProjectsOverview: () => invoke<ProjectOverview[]>("list_projects_overview"),
```

- [ ] **Step 4: Type-check**

Run: `cd apps/desktop && pnpm build` (runs `tsc && vite build`)
Expected: compiles cleanly. This is a purely additive change (new optional-nowhere fields, new interfaces, new wrapper functions) — nothing existing imports or destructures these new fields yet, so no existing component should need any change to keep compiling.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/types.ts apps/desktop/src/lib/api.ts
git commit -m "Sync frontend types and API wrappers with the backend"
```

---

## Self-Review Notes

- **Spec coverage:** every field/interface/wrapper gap the frontend survey identified is addressed in this one task, except the deliberately-excluded `capture_handback` wrapper (see Global Constraints for why).
- **Explicitly deferred, not silently dropped:** all component-level consumption of these new fields (5-state visuals, session strip, etc.) is out of scope — later frontend phases.
- **Type consistency check:** every new field name/type in `types.ts` matches the Rust struct it mirrors exactly (verified against `crates/magpie-core/src/model.rs` and `crates/magpie-core/src/projects.rs` during planning); `api.ts`'s two new wrappers use the exact Tauri command names already registered in `apps/desktop/src-tauri/src/lib.rs` (`list_sessions`, `list_projects_overview`).
