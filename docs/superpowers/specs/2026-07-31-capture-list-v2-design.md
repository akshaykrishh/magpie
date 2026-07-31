# Capture List v2 — Sections, Interaction Model, Markdown, Shortcuts, Deletion

Extends `docs/design.md`. Where this doc is silent, that one governs.

## Context

The stream, Now, and Templates views currently offer almost no organization or
interaction beyond hover-icon buttons (Done/Promote/Demote) and the multi-select
merge toolbar. This batch adds: **Sections** (manual, ordered grouping),
a **context menu** replacing the hover icons, **Copy / Copy as List**, inline
**Edit**, **Markdown rendering**, **Custom (global hotkey) shortcuts**,
**keyboard-first navigation**, and **deletion** (currently entirely absent for
captures and projects, and template deletion is instant/unrecoverable).

## Scope & phasing

One design, six sequential, independently-shippable phases:

1. Data model foundation (`sections`, `deleted_at`, migration `0008`)
2. Deletion (soft-delete + Undo + Recently Deleted) — ships the safety net before
   later phases can accidentally destroy anything
3. Markdown rendering
4. Interaction model overhaul (context menu: Copy / Copy as List / Edit / Expand /
   Move to Project / Move to Section / Merge / Delete)
5. Keyboard-first navigation (built on Phase 4's action handlers)
6. Custom Shortcuts settings window

**Explicitly out of scope:** deleting a Project (its own design conversation —
what happens to its captures/Now items/sessions is a separate hard question);
per-action user-remappable keyboard shortcuts (only the two global OS hotkeys
are remappable; in-app action keys get fixed, well-chosen defaults).

## Data model

New migration `crates/magpie-core/migrations/0008_sections_and_soft_delete.sql`
(next free number after `0007_session_digests`), registered in `db.rs`'s
`MIGRATIONS` array:

```sql
CREATE TABLE sections (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    position REAL NOT NULL,   -- fractional index, same technique as captures.queue_pos
    created_at TEXT NOT NULL,
    deleted_at TEXT           -- soft-delete, see "Deletion" below
);

ALTER TABLE captures  ADD COLUMN section_id INTEGER REFERENCES sections(id);
ALTER TABLE templates ADD COLUMN section_id INTEGER REFERENCES sections(id);
ALTER TABLE captures  ADD COLUMN deleted_at TEXT;
ALTER TABLE templates ADD COLUMN deleted_at TEXT;
```

Two independent nullable FKs into one shared `sections` table — not a generic
polymorphic association. There are exactly two owners (captures, templates);
a polymorphic `owner_type`/`owner_id` design would be solving a generalization
problem that doesn't exist yet.

**No `section_pos` column.** A section is a *visual grouping* layered over
whatever ordering already governs the view it's rendered in — `created_at DESC`
in the stream, `queue_pos` ASC in Now. Items don't need a second, competing
position dimension; only the section *headers themselves* need their own order
(`sections.position`, fractional, reusing the exact MIN/MAX-neighbor technique
already proven in `captures.rs::reorder`/`promote` — no new algorithm to write).

**Single membership, not many-to-many.** A capture/template has at most one
`section_id`, unlike tags (`capture_tags` is already a real many-to-many join
table, confirmed working and tested today — `tag_untag_round_trip`). This is
the actual distinction between a Section and a Tag: cardinality and ordering,
not which view it appears in.

**Sections are global, not project-scoped** — like tags, unlike Now. A
capture or template's `project_id` (or lack of one) is irrelevant to which
section it can join.

**Sections never reach MCP.** `queue_take`/`queue_peek` continue to be governed
purely by `queue_pos`, exactly as today. No section-aware MCP tool is added —
this preserves `docs/design.md`'s "Agent trust" design (the MCP surface is
deliberately minimal and non-destructive against a prompt-injection threat
model; every new capability considered for this batch was checked against
that model and none of them are exposed to MCP).

**Existing queries gain a filter.** `list_stream`, `list_now`, `list_templates`,
and `search` all add `WHERE deleted_at IS NULL`. A new `list_recently_deleted`
(captures and templates) does the inverse, ordered by `deleted_at DESC`.

## Sections — rendering & management

**No explicit "Unsectioned" header.** Items without a `section_id` render in
their existing order (`created_at DESC` in the stream, `queue_pos` in Now) —
visually identical to today if you've never created a section. Section
groups render above that plain list, each in the section's own fractional
`position` order — the deliberately-organized stuff floats to a defined
position, same shape as the dock's existing "one project focused, the rest
peripheral" pattern in `docs/design.md`.

**Search ignores section grouping entirely.** `search_captures` returns its
existing FTS5-ranked flat list, exactly as today. Grouping by section during
search would bury the actual relevance ranking search exists to surface,
behind organizational structure that's irrelevant to "did I find the right
one."

**Management is in-place only — no dedicated "Manage Sections" panel.**
Rename, drag-to-reorder, and delete all live directly on the section header
wherever it's rendered (stream, Now, Templates), matching how `NowList`'s
drag-reorder and `MergeToolbar`'s inline bar already work in this app. A
separate management screen would just be a second, partially-redundant way to
make the same edits.

**Session digests can join a section and be copied normally.** Only Edit is
restricted for `session_digest` rows (see "Edit" above) — their body is
ordinary text, so Copy, Copy as List, and section assignment all behave the
same as any other capture.

## Deletion

Soft-delete via `deleted_at`, for both captures and templates (templates'
existing instant hard-delete via `delete_template`/`Trash2` is upgraded to
match — one consistent delete behavior across the app, not two).

- **Delete** (single item from the context menu, or the whole checked
  multi-selection) sets `deleted_at = now_iso()` and hides the item(s) from
  every view and from search immediately.
- **Undo** is a real, clickable, **in-window React toast/snackbar** — not a
  reuse of the ambient capture-confirmation toast. That toast (`toast.rs`) is
  a deliberately non-activating, click-through `NSPanel`
  (`can_become_key_window: false`, shown via `orderFrontRegardless`) built so
  an ambient capture confirmation never steals focus while you work elsewhere.
  It cannot host a clickable button — clicks pass through it. Delete happens
  with the main window already focused (you right-clicked something visible
  in it), so Undo is an ordinary interactive toast component rendered inside
  the existing React app, unrelated to the capture-toast mechanism.
- **Recently Deleted** is a new view (alongside Captures/Templates/Activity)
  listing both deleted captures and templates, most-recently-deleted first,
  each with **Restore** and **Delete Permanently**.
- **Merge interaction:** merging creates a new capture whose absorbed sources
  point to it via `merged_into` (already excluded from every view/export —
  see `export_excludes_merged_away_captures`). Soft-deleting a merged-result
  capture cascades `deleted_at` to its absorbed sources at the same time, and
  restoring it un-cascades the same way — otherwise Undo would restore a
  capture whose merge history silently vanished into orphaned, invisible rows.
- **Section deletion** goes through the same soft-delete/Undo path as captures
  and templates, for consistency (nothing in this batch gets a special-cased
  instant-delete). Deleting a section only clears `section_id` on its members
  (`SET section_id = NULL`) — it never touches the members' own `deleted_at`.
- **Purge sweep** hard-deletes anything with `deleted_at` older than ~30 days,
  cascading blob/tags/FTS exactly like the existing tested cascade behavior
  (`deleting_a_screenshot_capture_removes_its_blob_and_search_entry`). Runs
  **both** at app startup (alongside the existing dead-pid lease sweep) *and*
  on a recurring ~24h interval while the app is running — startup-only would
  never fire for a tray app that stays open for weeks, which this one is
  designed to do.
- **No `delete_capture`/`delete_template`/`delete_section` MCP tool is ever
  added.** `docs/design.md`'s "Agent trust" section documents the MCP surface
  as deliberately non-destructive (no delete, no export, no shell) as the one
  mitigation that holds regardless of whether a prompt injection succeeds.
  This batch does not weaken that boundary anywhere.
- **No confirmation dialog on delete**, single or batch. Soft-delete + a real,
  working Undo is the modern replacement for "Are you sure?" modals (Gmail,
  Superhuman, Things) — confirmation dialogs get reflexively dismissed and
  provide less actual safety than an undo that works.

## Markdown rendering

Renders everywhere a body is currently shown as raw text: stream, Now, the new
Expand/detail view, and template bodies. The transient capture-confirmation
toast stays plain text (nothing to format in a 1.8s confirmation).

**Security requirement, not a detail left to implementation:** captured
content routinely originates from arbitrary clipboard content off untrusted
web pages. Render with **`react-markdown` and no `rehype-raw` plugin** —
`react-markdown` is safe-by-default and renders embedded HTML as inert text
rather than executing it. Enabling raw-HTML passthrough (the default in some
other markdown libraries) would be stored XSS inside the Tauri webview the
moment someone captures a snippet containing a crafted `<img onerror=...>`
from a webpage. Treat captured content as untrusted here for the same reason
`docs/design.md`'s "Agent trust" section already treats it as untrusted for
MCP.

**Performance note, not a new feature in this batch:** the stream is a plain
`.map()` over an array with no list virtualization today, and `docs/design.md`
already states "seed 10k captures; search stays responsive and the list
doesn't jank" as a verification bar. Markdown parsing is heavier per row than
plain text. This batch does not add virtualization (out of scope, not asked
for) — flagging that if 10k-capture streams start to jank once markdown
parsing lands, virtualization is the fast-follow, not a surprise.

## Edit

Plain `<textarea>` swapped in for the body in place; saving re-renders through
the same Markdown path. No live split preview, no WYSIWYG toolbar — consistent
with the rest of the app's low-chrome style. "Edit in New Window" opens the
same editor in a separate Tauri window.

**Session digests:** `captures.kind = "session_digest"` rows are system-
generated end-of-session summaries. The codebase already restricts a mutating
action on them (`promote_rejects_a_session_digest`). Following that existing
precedent: **Edit is disallowed on session digests** (they're a system record,
not user-authored content); **Delete is allowed** (soft-delete, same as any
other row — dismissing an unwanted digest is reasonable and, being
soft-deleted, undoable).

## Context menu

Replaces the hover-icon row (Done/Promote/Demote) entirely. Triggered by
right-click, or a hover-reveal `•••` button on the row (matching how the
icons it replaces already behaved — a persistent per-row control on every row
at rest is visual noise a minimalist list shouldn't pay for; right-click
itself isn't hover-gated, since the whole row is already the click target,
and keyboard users get the dedicated shortcut below, also not hover-gated),
or a keyboard shortcut that opens the identical menu on the currently
highlighted row (see Keyboard-first navigation) — one action set, three entry
points, not three separately-maintained ones.

**Batch actions are primarily reached through `MergeToolbar`, extended.**
The existing `MergeToolbar` (appears only when 2+ items are checked) absorbs
the *whole* batch action set — Copy as List, Move to Project, Move to
Section, Delete — not just Merge. It's already conditional/appears-only-
when-useful, so this costs nothing in visual weight at rest, and it's more
discoverable than requiring a right-click on a selected row to reach batch
behavior, which nothing in the UI otherwise hints at. Right-click on a
selected row still reaches the same batch actions too — the toolbar is the
obvious path, right-click is the secondary one, not a replacement for it.

Full action set: Mark Done / Reopen (toggles with state), Promote to Now /
Remove from Now (toggles with state), Copy, Copy as List, Edit, Edit in New
Window, Expand, Merge Notes (enabled at 2+), **Move to Project** (submenu:
your projects + Inbox), **Move to Section** (submenu: your sections + "New
section…" + "None") — two separate entries, not one combined submenu, since
project and section are different axes — and Delete.

## Copy / Copy as List

Nothing in the desktop app writes to the system clipboard today (only reads,
for capture) — this is new infrastructure, via the official
`tauri-plugin-clipboard-manager` plugin (new dependency).

**Single-item Copy on a screenshot copies the actual image bytes**
(`writeImage`), not derived text — matching how every native macOS app
(Finder, Preview, Chrome's "Copy Image") already treats "Copy" on an image.
A text capture's Copy stays plain body text, as before.

**Copy as List is inherently textual**, so it can't carry a real embedded
image the same way — no plain-text/Markdown format can (Markdown's image
syntax needs a hosted URL, which a local screenshot doesn't have; this isn't
a magpie-specific limitation). It copies as a **Markdown checklist** —
`- [ ] <body>` per line — rather than a plain bullet list, the deliberate
differentiator asked for: directly actionable when pasted into a GitHub
issue, PR description, or an agent prompt, and it dogfoods the Markdown
feature already in this batch. A screenshot's line uses `blob.ocr_text` when
`body` is empty, or an honest `[screenshot — OCR pending]` placeholder in the
rare case OCR hasn't finished yet (a small window — `capture_flow.rs` already
runs OCR within "a real fraction of a second" of capture) — never a silent
blank line. (Considered and rejected: disabling Copy as List entirely
whenever a screenshot is selected — throws away the common case of OCR text
already being ready, for a narrow timing window that barely ever occurs in
practice.)

## Keyboard-first navigation

Gmail/Superhuman model, not a "cursor position is the selection" model.

**Focus mechanism: real DOM focus, not a hand-rolled "active pane" variable.**
The two-pane layout (Now sidebar always visible, stream/Templates in the main
area) means multiple lists can be on screen at once — arrow keys need to know
which one they apply to. Each list container gets `tabIndex={0}` and
`:focus-within` styling; clicking anywhere in a list (including its empty
background, not just a row) gives it real browser focus, and Tab cycles
between Now / stream / search / Templates like any keyboard-accessible web
app. This is the boring, proven roving-tabindex/listbox pattern — it comes
with Tab-key pane-switching and correct screen-reader focus announcement for
free, instead of a custom JS variable that has to be kept in sync with
whatever's visually focused by hand.

- Arrow Up/Down move a visual highlight **cursor**, independent of the
  checkbox multi-select.
- **Space** toggles the checkbox on the cursor row (builds the multi-selection
  without moving the cursor).
- **Enter** expands the cursor row.
- Single-key shortcuts mirror the context menu (`d` done, `c` copy, `⇧c` copy
  as list, `e` edit, `⇧m` merge, `⌫` delete, etc.) and a dedicated shortcut
  opens the same context menu programmatically (parity with right-click for
  keyboard-only/screen-reader users, rather than only ever exposing
  individually-mirrored keys that could drift out of sync with the menu).
- An action key acts on the checked multi-selection if non-empty, otherwise
  the cursor row — same rule the context menu uses for right-click.
- **Critical scoping rule:** none of these single-key shortcuts fire while
  focus is inside any `<input>`, `<textarea>`, or contenteditable element —
  `AddPromptInput`, `SearchBar`, and the new inline-edit textarea all need
  this respected, or typing the letter "d" while composing a new capture would
  fire "mark done" instead of typing a "d".
- These are fixed defaults, not user-remappable (see Custom Shortcuts below).

## Custom Shortcuts

A new Settings window remaps only the two existing global OS-level hotkeys —
capture (`⌘⇧M`) and screenshot (`⌘⇧⌥M`), currently hardcoded consts in
`lib.rs`. In-app action shortcuts (the keyboard-nav list above) are **not**
remappable in this batch — building a full configurable-keymap system
(persistence, conflict detection against both OS and in-app bindings, a
rebind-capture UI) for shortcuts nobody has used yet is scope not worth
carrying; the two global hotkeys are the ones that actually collide with
other apps and are worth the investment.

- Persisted in a new `settings(key TEXT PRIMARY KEY, value TEXT)` table (one
  row per hotkey), read at startup before shortcuts register.
- Rebinding at runtime unregisters the old `Shortcut` and registers the new
  one via `tauri_plugin_global_shortcut`, no app restart required.
- **Must require at least one modifier key.** Binding either hotkey to a bare
  unmodified key (e.g. plain `A`) would break normal typing system-wide;
  reject that in the settings UI before attempting registration.
- **Must handle registration failure explicitly.** `tauri_plugin_global_shortcut`'s
  register call can fail if another app already owns that combination — OS-level
  conflicts aren't detectable in advance. The settings UI must surface that
  failure and leave the old binding active, not silently report success for a
  rebind that didn't actually take effect.

## Error handling

- All new Tauri commands follow the existing `CmdResult<T>` / `map_err`
  pattern in `commands.rs` — no new error-handling convention introduced.
- **Acting on an already-soft-deleted row needs no new error variant.** The
  codebase already has a tested `CaptureNotFound`/`TemplateNotFound`
  convention (`error.rs`, used consistently by `mark_done`/`promote`/
  `reorder`/etc.). Every id-based lookup simply adds `deleted_at IS NULL` to
  its `WHERE` clause, so a soft-deleted row falls through to the exact same
  `NotFound` error a genuinely-missing row already produces — covering the
  real race where one window deletes an item while another still holds a
  stale reference to it (e.g. from before its last refresh).
- Assigning a `section_id` that doesn't exist (deleted between menu-open and
  click, e.g. via a concurrent GUI window) returns an error rather than
  silently creating a dangling reference.
- Restoring a capture/template whose section was deleted in the meantime
  restores with `section_id = NULL` (already cleared by the section's own
  soft-delete cascade above) rather than erroring.

## Cross-window sync

The main window and pinned dock already sync via plain Tauri broadcast
events (`now:changed`, `capture:updated` — see `apps/desktop/src/lib/events.ts`).
Every new mutation in this batch (delete/restore, section create/rename/
reorder/delete, section/project (re)assignment, edit) follows the same
pattern — no new sync mechanism invented. Exact event names/payloads are an
implementation-plan detail, not a design-level decision.

## Testing / verification

- **Core:** soft-delete/restore round-trip for captures, templates, and
  sections; purge sweep only removes rows past the threshold, both at startup
  and on the recurring timer; section assignment enforces single membership
  (reassigning clears the previous `section_id`); deleting a section clears
  members' `section_id` without touching their `deleted_at`; deleting a
  merged-result capture cascades to its absorbed sources and restoring
  un-cascades; FTS/search excludes soft-deleted rows; existing captures with
  no markdown syntax render identically to their plain-text form (no visual
  regression for content that predates this feature).
- **Security:** a capture body containing raw HTML (e.g. `<img onerror=...>`)
  renders as inert text, not executed, confirming `rehype-raw` is absent.
- **UI:** context-menu and `MergeToolbar` batch behavior (2+ selected vs.
  single, both entry points reach the same actions); keyboard cursor/checkbox
  independence (arrow keys never mutate the selection, Space never moves the
  cursor); Tab cycles focus between Now/stream/search/Templates and arrow
  keys apply to whichever list currently holds real DOM focus; single-key
  shortcuts are inert while any text input has focus; Custom Shortcuts
  rebind takes effect without restart, and a deliberately-conflicting rebind
  surfaces a visible failure rather than a false success; single-item Copy
  on a screenshot places real image bytes on the clipboard, pasteable into
  another app as an image.
- **Concurrency (matches existing critical-test tier in `docs/design.md`):**
  two GUI windows/processes racing a section delete against an item promotion
  don't corrupt state, consistent with the existing `BEGIN IMMEDIATE` leasing
  discipline already relied on for `queue_take`.
