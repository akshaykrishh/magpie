# Magpie — an open-source capture tool for AI-assisted work

**License:** GPL-3.0 · **Repo:** personal GitHub account · **Platforms:** macOS + Linux
*Name is a placeholder — rename before the repo goes public if you find better.*

## Context

Working with AI means constantly collecting small things you don't want to lose — an answer
worth keeping, a link, an idea, three follow-up prompts that occur to you while the current one
is still generating. They scatter across ChatGPT, Claude, Cursor, and Chrome.

Existing tools in this space tend to be Mac-only, paid, closed, and — by deliberate design — not
integrated with anything: a buffer only human hands can move things into and out of.

This project targets the same problem with two structural advantages a closed, human-only tool
can't follow without becoming a different product:

1. **It runs on Linux**, not just macOS.
2. **Agents can reach the capture list.** An MCP server makes the queue readable and writable by
   Claude Code, Cursor, and anything else speaking MCP. Captures become work items in an agent
   loop rather than dead notes.

The second is the moat, and it's architectural rather than effortful. A local, private,
no-account, deliberately simple tool structurally can't follow it without becoming something
else.

**Design center: the agent developer.** Terminal-native, runs Claude Code or Cursor agents,
macOS or Linux. The full feature set still serves the broader AI-prosumer audience, but when a
decision can only go one way, developers break the tie. Rationale: the MCP wedge is worthless to
someone living in a browser; open-source distribution works through developer channels; and
Linux only matters to this audience. Centering on developers still reaches prosumers — the
reverse doesn't hold.

## Architecture

A Rust core owns the domain model. GUI, CLI, and MCP server are thin clients of it. This is what
makes the moat cheap — the MCP server is a few hundred lines because it reuses everything.

```
crates/
  magpie-core/       domain model, SQLite, FTS, merge, templates, export, audit
  magpie-capture/    CaptureBackend trait + macOS / Linux impls, Windows stub
  magpie-cli/        binary: magpie
  magpie-mcp/        binary: magpie-mcp   (rmcp, stdio)
apps/desktop/
  src-tauri/         Tauri v2 shell — tray, windows, hotkeys, IPC
  src/               React + Tailwind + shadcn/ui
```

### Storage — SQLite, WAL, FTS5

Source of truth. Industry standard for this shape of app (Apple Notes, Bear, Things, Raycast,
Signal, VS Code). Three reasons it's decisive here:

- **Concurrency.** GUI, CLI, and multiple MCP servers write simultaneously — an agent draining
  the queue while you type is the normal case. WAL handles multi-process access correctly.
- **Search.** FTS5 ranks 10k captures in milliseconds.
- **Relational data.** Queue order, leases, merge, and project scoping are joins and transactions.

Own-your-data is honoured through transparency, not worse storage: unencrypted `.db` at a
conventional path (`~/Library/Application Support/magpie/`, `~/.local/share/magpie/`), schema
documented in-repo, and a real `magpie export --format md|json`. No live Markdown mirror —
two-way sync is a bug farm.

```sql
projects(id, name, remote_url, common_git_dir)        -- identity from git remote
sources(id, app_name, bundle_id, window_title, url, captured_at)
captures(id, body, created_at, done_at, failed_reason,
         queue_pos NULLABLE,      -- non-null ⇒ in Now; value is order
         project_id NULLABLE,     -- null ⇒ Inbox
         branch NULLABLE,         -- non-null ⇒ only matching sessions may take
         lease_session NULLABLE, lease_client, lease_pid, lease_at,
         source_id, merged_into)
templates(id, title, body, created_at)                 -- persist; instantiate into Now
tags(id, name)  /  capture_tags(capture_id, tag_id)
blobs(id, capture_id, path, mime, width, height, ocr_text)
audit(id, at, actor, action, capture_id)               -- every MCP action, shown in GUI
captures_fts(body, ocr_text)                           -- FTS5
```

Screenshots live on disk in `blobs/`, path stored in a row. Merge creates a new capture whose
body is the concatenation, children pointing at it via `merged_into` so each keeps its provenance.

### Data model — one stream, one working set

**Everything lands in one reverse-chronological stream.** Search and tags span all of it. Nothing
is ever auto-filed, because auto-filing that's 85% right creates more work than none — every item
then needs auditing.

**`Now` is a short ordered working set you promote into deliberately** (`queue_pos` non-null).
Typing a prompt into the panel goes straight to `Now`; anything captured from elsewhere lands in
the stream and one keypress promotes it. The dock shows `Now`. If you never promote anything, it
degrades to a plain capture list, which is fine.

**Templates persist and get instantiated; captures drain.** A reusable prompt isn't a work item —
it's a stencil. "Run on nexa-erp" copies it into that project's `Now` while the original stays in
the library. This is what makes the tool work across several projects, which is why templates
moved earlier than originally planned.

```
Capture (Inbox, no project)
   ├── assign to project ──→ that project's stream
   ├── promote to Now ─────→ work item, drains, agent-visible
   └── save as template ───→ library, cross-project, permanent
                                └── instantiate → any project's Now (several at once)
```

### Projects and multi-session

**Project identity is the git remote URL**, falling back to the common git dir. This unifies
worktrees (same repo, different checkouts, one backlog), survives re-cloning to a different path,
and survives renames.

- **`Now` is project-scoped** — handing work to an agent in the wrong repo is a correctness
  failure, not a UX wrinkle. The MCP server derives its project from its own spawn directory, so
  this needs zero configuration.
- **The stream stays global** — you don't remember which project you were in when you saved that
  snippet. Project is a filter, never a wall.
- **Optional `branch` constraint** — null by default; non-null means only sessions on that branch
  or worktree can take the item.
- **Projects are not tags.** A typo in a tag would silently mis-scope agent work. Tags stay for
  cross-cutting themes (`#bug`, `#refactor`).

Several sessions in one project work by construction: `queue_take` leases inside
`BEGIN IMMEDIATE`, so SQLite serializes writers and two agents can never take the same item.

**Dock: one project focused, the rest peripheral.**

```
─ nexa-erp ──────────────────────
  ▸ fix OAuth redirect  claude#1 · 40s
  ▸ update tests        claude#2 · 12s
  ◦ migrate schema
─ elsewhere ─────────────────────
  salon-platform  3 queued · working
  ghost           1 queued
```

Auto-follows when one session is live; clickable to switch when several are. Degrades to a plain
list when nothing is running.

### Capture and permissions

**Ships working with zero permissions.** Copy, then hotkey. No dialog on first run, no System
Settings detour — the user reaches the good part in ten seconds.

**Progressive upgrade.** After the user has captured a few things, offer one-key capture in
exchange for Accessibility. Consent from someone who's felt the friction is consent they don't
resent. Never ask for a frightening permission before demonstrating value.

**One-key uses synthesize-copy, not the accessibility tree.** Post Cmd/Ctrl+C to the focused app,
verify it worked, restore the previous contents. `AXSelectedText` works well in native Cocoa apps
and fails in Chrome, Cursor, and Electron — exactly where these users live. Synthesize-copy is
less code and better coverage. **No AX code in v1**; add per-app fast paths later only if
something misbehaves.

**Verification uses a sentinel, not the pasteboard's change count.** Confirmed during
implementation: `changeCount` is a shared global counter, so anything else touching the clipboard
while polling (another app, a clipboard-history tool) can bump it without the synthesized
keystroke having done anything — a false positive. Writing a unique sentinel value first and
checking whether the pasteboard holds something *other than* that sentinel afterward is
unambiguous instead.

**Terminal emulators don't respond to synthesize-copy at all — confirmed against real
Terminal.app and Ghostty sessions, not a theoretical gap.** The synthetic Cmd+C posts cleanly, no
error, but the pasteboard never changes even with real text selected — these apps implement their
own keyboard handling for PTY passthrough rather than going through the Cocoa responder chain a
synthetic key equivalent relies on (which is what Notes, Arc, and other Cocoa-text-view apps do
use, and where synthesize-copy works correctly). When synthesis is confirmed to have failed, the
backend falls back to reading whatever's already on the clipboard — picking up a manual Cmd+C the
user pressed themselves, i.e. transparently degrading to the zero-permission flow for exactly the
apps where the upgrade can't work, while every other app keeps one-key capture.

**Freshness tracking lives in the backend, not the app layer, and is seeded at construction.**
Both fallback (above) and the plain clipboard path need to distinguish "the user just copied
something new" from "this is whatever happened to already be on the clipboard." A backend that
only remembers its *previous* capture misses content that predates the app starting — an early
implementation reported a stale sentence from an unrelated earlier clipboard action as a fresh
capture the first time the hotkey was pressed. The fix: every backend hashes whatever's on the
clipboard at construction time as its baseline, and `read_capture_text` only ever returns content
whose hash differs from the last one it saw.

**Focus model — three distinct behaviours:**

| | Behaviour |
|---|---|
| Capture | Click-through "Captured ✓" toast. Never takes focus. |
| Compose | Separate hotkey opens the panel, which takes focus normally — you're typing into it. |
| Return | Record the frontmost app before showing; reactivate on Esc or fire. |

Making capture a toast rather than a full panel dissolves most of the difficulty, and suits a
developer capturing from a terminal better than a panel covering their output.

**Provenance is tiered and degrades honestly:**

- Free — app name, bundle id, timestamp. "Captured from Cursor" is most of the value.
- With Accessibility — focused window title, via a bounded AXUIElement chain (app → focused
  window → title; one attribute read, a 1s messaging timeout, no general tree traversal). This is
  what disambiguates browser tabs, which are otherwise invisible to provenance entirely:
  NSWorkspace reports "Chrome" whether the tab is chatgpt.com, claude.ai, or a GitHub issue, since
  a browser is one application as far as the OS is concerned. ChatGPT, Claude, GitHub, and Stack
  Overflow all set informative tab titles, so this answers "which page was this from" — the actual
  ask — without needing the exact URL. Needs no permission beyond the Accessibility already
  required for one-key capture, and no extra user action. Considered and rejected: a browser
  extension (a second thing to install and maintain across two independently-versioned browser
  stores) and a bookmarklet (still an extra click every time) — both solve the same problem for
  strictly more cost than reading a title the OS already tracks.
- **Never prompt for Automation.** Exact browser URLs need per-browser AppleScript consent
  ("Magpie wants to control Google Chrome") — a third scary dialog, for a URL the window title
  mostly makes unnecessary. Not planned.

`capabilities()` reports what the running platform can actually deliver, and the UI adapts rather
than showing fields it can't fill.

### Platform reality

| | macOS | Linux / X11 | Linux / Wayland |
|---|---|---|---|
| Read selection | synthesize Cmd+C (Accessibility) | synthesize Ctrl+C, or PRIMARY (no permission) | **clipboard-watch only** — synthetic input is blocked by design |
| Global hotkey | `NSEvent` global monitor | `XGrabKey` | Portal `GlobalShortcuts`: **KDE only**; wlroots ships none; GNOME in progress |
| Non-activating window | `NSPanel` via `tauri-nspanel` | override-redirect | layer-shell — unproven through Tauri's GTK layer |
| Tray | `NSStatusItem` | SNI | SNI (GNOME needs an extension) |
| Provenance | app + title | app class + title | often nothing; `wlr-foreign-toplevel` where present, not on GNOME |

Two things worth stating plainly. **X11 Linux is the easiest of the three targets** — PRIMARY
selection means no permission at all. And **on Wayland, clipboard-watch is the primary path, not
a fallback.** Anything claiming full Wayland hotkey support today is overselling. The universal
clipboard-watch path is built first, so every platform works from day one.

### MCP contract

Uses [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk), the official Rust SDK, over stdio.

```
queue_peek(n)          read-only, for planning across items
queue_take()           leases exactly ONE item
capture_done(id)
capture_fail(id, why)  visible and actionable, not a zombie
capture_add(text)
capture_search(query)
```

**Lease and acknowledge, with no auto-expiry.** SQS-style visibility timeouts exist because
nobody is watching; here there are three items and a human staring at a dock. Worse, timeouts
pair with *idempotent consumers* — and an LLM making side-effectful changes to a codebase is the
least idempotent consumer imaginable. A lease expiring mid-refactor means running the refactor
twice. For non-idempotent consumers, at-most-once with human-initiated retry is correct.

**Recovery uses liveness, not timers.** The MCP server is a child process of the agent host, so
when the agent dies its stdio closes — release leases there. It fires when the consumer is
genuinely dead, never merely slow. Back it with a dead-pid sweep on GUI startup for `kill -9`.

**Lease one item, not a batch.** Agents work serially; leasing three means two sit leased-but-idle
and the dock reports work nobody has started. State that lies is worse than no state. `queue_peek`
covers planning without mutating.

**Attribution** comes from MCP's `initialize` handshake (`clientInfo`), so the dock shows
"claude-code · 2 min" rather than a generic in-progress state.

**Push is the CLI's job**, not MCP's: `magpie drain --tag refactor | claude -p`.

### Agent trust

The MCP server pipes arbitrary captured web content into a tool with shell access — the standard
prompt-injection trifecta, assembled by design. Nobody has solved this; shrink the blast radius
and be candid.

- **Non-destructive surface.** No delete, no export, no file paths, no shell. Worst case from a
  successful injection is a junk capture. This is the mitigation that holds, because it doesn't
  depend on the model behaving.
- **Use the existing trust boundary.** `Now` items were typed or deliberately promoted by a human
   — that promotion *is* review. Stream content was never looked at. Tool responses say which.
- **Label and delimit** returned content with its provenance, marked as data rather than
  instructions. Weak alone, free to add.
- **Audit log** of every MCP action, visible in the GUI. Turns "what did the agent do while I was
  at lunch" into a scroll, and makes the whole integration feel trustworthy.
- **README documents the risk plainly.** Most MCP servers say nothing; being straight about it is
  a credibility win.

## Milestones

Ordered by dependency, no calendar. Every tag is an installable, usable app.

### M0 — focus spike (before anything else)

A throwaway Tauri app that shows a click-through toast on a global hotkey **while you keep typing
in another app**, on macOS and on Linux. Proves `tauri-nspanel` and the Wayland/X11 paths before
the core exists. If this fails, the stack choice is wrong and you want to know in week one.

### M1 — core + capture loop → `v0.1`

- `magpie-core`: schema, migrations, CRUD, FTS, merge, export, projects
- `magpie-capture`: trait + clipboard-watch (all platforms) first, then synthesize-copy
- Tauri shell: tray, toast, panel, pinned dock with focused/peripheral projects
- Stream, `Now`, promote, drag-reorder, done, multi-select merge, tag filter, search
- Provenance tiers + `capabilities()`-driven UI
- Progressive permission flow
- Packaging: `.dmg` + Homebrew cask; `.AppImage` + `.deb`

Focus behaviour and permission-denied states are the risky parts — build them early, not last.

### M2 — MCP + CLI + templates → `v0.2`

- `magpie-mcp` with the contract above; lease lifecycle, audit log, stdio-close recovery
- `magpie` CLI: `add|list|search|done|drain|export|serve-mcp`, pipe-friendly
- Templates: create, edit, instantiate into one or several projects
- Project auto-detection from spawn directory; branch constraint

### M3 — retired, folded into capture provenance (done)

Originally planned as a browser extension for exact URL/title/selection with zero OS permissions.
Reconsidered: an extension is a second thing to install and maintain across two
independently-versioned browser stores; a bookmarklet (the lighter alternative also considered)
is still an extra click every time. Neither was worth it for what turned out to be gettable for
free — window-title reading via the Accessibility permission already required for one-key
capture (see "Provenance is tiered" above) answers "which page was this from" without any new
install, permission, or user action, at the cost of the exact URL, which the title mostly makes
unnecessary. Shipped as part of `magpie-capture`'s macOS backend rather than as its own milestone.

### M4 — screenshots + OCR → `v0.4` (macOS done and verified; Linux implemented, unverified)

Region-capture hotkey → blob on disk → capture row. OCR into `blobs.ocr_text`, FTS-indexed so
screenshots are searchable.

On macOS, region selection shells out to `screencapture -i` -- the same interactive picker behind
Cmd+Shift+4 -- and OCR runs through the Vision framework (`VNRecognizeTextRequest`). Vision
returns one observation per text region rather than one per visual line, so results are grouped
by each observation's bounding box (vertical center within half a text-height counts as the same
line) before being joined, rather than assuming enumeration order matches reading order.

On Linux, region selection goes through the `org.freedesktop.portal.Screenshot` portal interface
(`interactive: true`) -- the standardized mechanism across X11 and Wayland, GNOME and KDE alike,
rather than a hand-rolled X11 selection overlay. OCR shells out to `tesseract` if it's present on
`PATH`, reported honestly through `capabilities()` when it isn't. This path type-checks against
the `aarch64-unknown-linux-gnu` target but has not run on a real Linux desktop -- see the note at
the top of `magpie-capture/src/linux.rs`.

### M5 — git prompt packs → `v1.0` (done)

`magpie pack add github:someone/agent-prompts`, manifest at `magpie.json`. Packs are git repos —
no server, no accounts, no hosting, no moderation to run. Named `pack add` rather than the `add`
sketched originally: that name was already taken by plain-text capture, and `magpie add
github:...` versus `magpie add "some text I copied"` would otherwise be indistinguishable.
`pack add` also accepts a full git URL or a local directory/file directly, the latter mainly for
testing a pack before publishing it.

A manifest prompt can declare `{{name}}` placeholders in its body, with optional per-variable
`description`/`default` metadata in a `variables` object. Instantiating a template with unfilled
placeholders leaves them as literal text rather than silently blanking them out — an honest signal
that something wasn't filled in, not a smaller version of what the prompt actually said. The GUI
shows a small fill-in form before running a template that has any; the CLI's `pack add` and plain
`instantiate_template` calls leave placeholders unfilled unless a caller explicitly supplies
values.

Re-running `pack add` against the same source updates that pack's templates in place (matched by
title) rather than duplicating them — pulling a pack's latest changes is just importing it again.

## Open-source scaffolding

Public repo from M1 — a repo nobody can see attracts nobody.

- **GPL-3.0.** Nobody can ship a closed fork and sell it as their own. Costs little for an
  end-user app since nobody embeds an application, and it stays reversible: you own the copyright,
  so you can relicense to MIT later, but not the other way round.
- **Reserve the name and logo in the README** — licenses govern code, not identity. Note that
  `magpie` is a common word with prior software use, so the mark is thin.
- **Push access is yours alone.** Contributors open PRs; you merge or close. Nothing about
  publishing changes that.
- `README.md` with a real GIF of the loop — it does most of the recruiting. Lead with the agent
  loop, not the pretty panel.
- `ARCHITECTURE.md`, `docs/schema.md`, `CONTRIBUTING.md` with working dev setup on both platforms
- CI: `fmt`, `clippy -D warnings`, tests, build matrix on macOS + Ubuntu
- **The Windows `CaptureBackend` stub is the flagship good-first-issue** — a clean trait, a
  documented stub, and a passing test harness is the best contributor magnet available.

**Signing:** develop with a stable self-signed identity so your own Accessibility grant survives
rebuilds. Buy the Apple Developer ID ($99/yr) and wire notarization into CI when cutting `v0.1`.
The reason isn't the scary dialog — macOS binds Accessibility grants to a signature, so unsigned
builds revoke the grant on **every update**, silently disabling one-key capture for the users who
upgrade most. Linux needs none of this.

## Verification

- **Core:** merge preserves child provenance · FTS returns ranked results · export round-trips ·
  migrations apply to empty and populated databases
- **Concurrency (critical):** integration test with CLI writes against a GUI-held connection under
  WAL, asserting no lost writes. The whole architecture rests on this.
- **Lease correctness (critical):** two concurrent `queue_take` calls never return the same item ·
  stdio close releases leases · dead-pid sweep recovers `kill -9` · branch-constrained items are
  invisible to non-matching sessions
- **Focus (critical, manual):** capture from ChatGPT in Chrome, Claude desktop, Cursor, and a
  terminal — the caret never leaves the source app and typing continues uninterrupted
- **Permissions (manual):** run with Accessibility denied on macOS; the app explains the gap and
  the clipboard path still works
- **Linux:** test X11 *and* Wayland, on both KDE and GNOME — `capabilities()` reports honestly and
  the UI adapts
- **MCP:** register with Claude Code and drive a full round trip — take, work, write back, done ·
  kill the agent mid-task and confirm the item recovers · confirm the audit log matches
- **Multi-project (manual):** agents in two projects and two sessions in one project; confirm no
  cross-project or cross-branch item ever leaks
- **Scale:** seed 10k captures; search stays responsive and the list doesn't jank

## Deferred

- **Windows** — trait stubbed, documented, opened as an issue
- **Flatpak** — its sandbox complicates clipboard and portal access, the one thing that can't be
  complicated
- **Automation-based browser URLs on macOS** — superseded by the extension
- **Sync of any kind** — local-only is the promise
