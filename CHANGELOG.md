# Changelog

All notable changes to magpie are documented here, hand-written, in
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format. This project
adheres to [Semantic Versioning](https://semver.org/) (see `RELEASING.md` for
the pre-1.0 caveat).

This file is hand-maintained, not auto-generated from commit messages or PR
titles. This maintainer's commit history is long-form reasoned prose, not
Conventional Commits — an auto-generator would flatten exactly what makes
those messages worth reading into a mechanical one-liner. One piece of
writing here serves three destinations: this file, the GitHub release body
(`scripts/extract-changelog.sh` slices out one version's section verbatim),
and the in-app "Release notes" link Settings → About shows for the installed
version.

## [Unreleased]

## [0.1.1] - 2026-08-04

### Security

- Fixed a command-injection vulnerability in `magpie pack add <source>`: an untrusted source
  string was passed straight into `git clone`'s argv, and git's `ext::<command>` transport let a
  crafted source (e.g. `ext::sh -c '...'`) run arbitrary shell commands — full RCE for a command
  whose entire purpose is installing packs someone else wrote and shared. `pack_source.rs` now
  requires an exact `https://`/`git@` URL before anything reaches `git clone`, with
  `GIT_ALLOW_PROTOCOL` as a second layer; 5 regression tests cover the exact payloads.
- Set a real Content-Security-Policy on the desktop app's webview (was `csp: null`). Not
  currently exploitable on its own — there's no script-injection path today — but closes the gap
  for the moment one gets introduced.
- Patched 6 Dependabot advisories in the docs site's build tooling (`postcss`, `sharp`,
  `fast-uri`) via `pnpm-workspace.yaml` overrides. Build-time-only; never reached the shipped
  desktop app.

### Fixed

- A failed stream load (`refreshStream()`) used to fail silently to the console — the UI looked
  identical to "you have no captures." Now shows a visible error banner with a Retry button.

### Known limitations

- **Linux is still not hand-verified on real hardware.** Carried over from `v0.1.0`, still true:
  the full test suite passes in CI on every push, but nobody has run the actual desktop app
  (tray icon, global hotkey, screenshot capture, in-app update) on a real Linux machine. See
  `RELEASING.md`'s manual hardware check — outstanding, not a checklist item skipped for a good
  reason.
- macOS builds are still unsigned and unpublished — code signing isn't wired up yet (see
  `ROADMAP.md`). Linux-only release.
- The `glib` dependency (via the Linux tray/menu chain: `tray-icon → libappindicator → gtk →
  glib`) has an open, medium-severity Dependabot advisory with no compatible patched version
  available without a major gtk-rs bump — tracked in #15, not fixed here.

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
