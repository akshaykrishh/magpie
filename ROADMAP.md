# Roadmap

Now / Next / Later, not dates — this is a solo-maintained project and a dated
roadmap is a promise this repo won't reliably keep. This file is a snapshot;
git history and open issues are the ground truth for what's actually landed.
See [docs/design.md](docs/design.md) for the architecture and the reasoning
behind each decision — this file is only about sequencing.

## Now

Usable today, on `main` unless noted:

- **Capture core** — the stream, `Now`, search (FTS5), merge, tags, projects
- **MCP server + CLI** — `queue_take`/`queue_peek`, `capture_add`/`done`/
  `fail`/`handback`, `capture_search`; the `magpie` CLI (`add|list|search|
  done|drain|export|serve-mcp`)
- **Screenshots + OCR** — region capture, searchable text (macOS verified;
  Linux implemented, not yet run on real hardware — see Next)
- **Prompt packs** — `magpie pack add github:someone/agent-prompts`,
  fill-in-the-blank templates
- **Confidence-aware capture filing** — the desktop toast proposes a
  destination project and commits it on a quick tap; never auto-files
- **Sessions, handback, and session digests** — an MCP-connected agent's
  session is a persistent record; `capture_handback` gives it a third
  outcome beyond done/fail; ending a session writes a searchable summary
  into the stream
- **Capture list v2** — sections, soft-delete with real Undo and Recently
  Deleted, Markdown rendering, a context-menu interaction model (Copy, Copy
  as List, Edit, Move to Project/Section, Merge, Delete), keyboard-first
  navigation, and remappable global hotkeys. Complete on
  `worktree-capture-list-v2`, pending merge to `main`.

## Next

- **Merge capture list v2 to `main`**
- **Across (⌘⌥K)** — the cross-project "what needs a look" rollup. The
  backend read (`list_projects_overview`) already exists and is exposed
  over Tauri; no UI consumes it yet. This is the actual next frontend
  surface, and several other things below are sequenced behind it rather
  than in parallel, because they'd otherwise ship with nowhere to be seen.
- **Real Edit-in-New-Window** — capture list v2 shipped this as an Expand-
  modal stub; a real second Tauri window is its own scoped follow-up.
- **Linux, on real hardware** — the full suite passes in CI, but nobody has
  run the desktop app itself on a Linux machine yet. X11 and Wayland (KDE
  and GNOME) both need first-hand verification before Linux can be called
  done rather than "type-checks."
- **Code signing and notarization** — CI ships unsigned artifacts today.
  Needed before a real v1: unsigned builds revoke macOS Accessibility
  grants on every update, silently breaking one-key capture for anyone who
  upgrades.
- **Agent session follow-ups** — a way for an MCP-connected agent to write
  back open items it noticed but didn't act on (`capture_followup`), folded
  into that session's digest rather than adding rows, surfaced as a count
  on Across and closed the same way anything else is closed. Designed, not
  started — deliberately parked until Across ships, since a write path
  with nothing surfacing it is clutter with the appearance of a feature.

## Later

Plausible, not scoped:

- **"Hold to aim"** — a keyboard-driven picker to redirect a filing guess to
  a different project, instead of only confirm-or-ignore
- **Deleting a project** — what happens to its captures, `Now` items, and
  sessions is a real design question capture list v2 deliberately didn't
  answer while shipping item-level deletion
- **List virtualization** — the stream is an unvirtualized `.map()` today;
  fine at current scale, a likely fast-follow once Markdown rendering and
  capture counts grow enough to jank
- **Windows** — `CaptureBackend` is trait-stubbed and documented; the
  flagship good-first-issue for a contributor

## Non-goals, for now

Considered and deliberately not doing, with the reasoning that got them here
— not a backlog, a record of what this project is choosing not to become:

- **Sync of any kind.** Local-only is the promise this app makes, not a
  missing feature.
- **Flatpak.** Its sandbox complicates the one thing that can't be
  complicated: clipboard and portal access.
- **Automation-based exact browser URLs on macOS.** Superseded by
  window-title provenance, which answers "which page was this from"
  without a third scary permission dialog.
- **Per-action remappable in-app keyboard shortcuts.** Only the two global
  OS hotkeys are remappable; a full configurable keymap for shortcuts
  nobody has used yet isn't worth the persistence/conflict-detection/UI
  cost until it is.
- **Auto-filing anything.** Not a gap — a standing architectural decision.
  Confidence-aware filing proposes and requires a tap; nothing in this app
  silently decides where a capture belongs.
