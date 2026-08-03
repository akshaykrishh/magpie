# Changelog

## [Unreleased]

## [0.1.0] - 2026-08-03

### Added

- Capture core — the stream, Now, search (FTS5), merge, tags, projects.
- MCP server + CLI — `queue_take`/`queue_peek`, `capture_add`/`done`/`fail`/`handback`,
  `capture_search`; the `magpie` CLI (`add|list|search|done|drain|export|serve-mcp`).
- Screenshots + OCR — region capture, searchable text (macOS verified; Linux implemented,
  not yet run on real hardware — see this release's Linux hardware check in `RELEASING.md`,
  a later chunk).
- Prompt packs — `magpie pack add github:someone/agent-prompts`, fill-in-the-blank templates.
- Confidence-aware capture filing — the desktop toast proposes a destination project and
  commits it on a quick tap; never auto-files.
- Sessions, handback, and session digests — an MCP-connected agent's session is a persistent
  record; `capture_handback` gives it a third outcome beyond done/fail; ending a session
  writes a searchable summary into the stream.
- Capture list v2 — sections, soft-delete with real Undo and Recently Deleted, Markdown
  rendering, a context-menu interaction model, keyboard-first navigation, and remappable
  global hotkeys.
- The "3a canonical" redesign — two-theme Slate/Paper token layer, the main window rebuilt
  around a session strip and one overlay stack, hold-to-aim, and Across (⌘⌥K).
- In-app update checking and installation — background and manual checks, auto-download,
  and user-initiated install and relaunch; never installs or relaunches without you seeing
  what's in the update first.

### Known limitations

- Linux builds are CI-verified but not yet hand-verified on real hardware for this specific
  release (tracked as a one-time manual check before this tag is treated as the real first
  release — see `RELEASING.md`, a later chunk).
- macOS builds are unsigned and unnotarized in this release — the Apple Developer ID isn't
  set up yet. Only the Linux artifact is published for `v0.1.0`.
