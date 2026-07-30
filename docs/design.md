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
extension/           browser extension (M3)
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
poll the pasteboard change count with a timeout, restore the previous contents. `AXSelectedText`
works well in native Cocoa apps and fails in Chrome, Cursor, and Electron — exactly where these
users live. Synthesize-copy is less code and better coverage. **No AX code in v1**; add per-app
fast paths later only if something misbehaves.

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
- With Accessibility — focused window title.
- **Never prompt for Automation in v1.** Exact browser URLs need per-browser AppleScript consent
  ("Magpie wants to control Google Chrome") — a third scary dialog. The browser extension (M3) is
  the real answer and needs no OS permission at all.

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

### M3 — browser extension → `v0.3`

Exact URL, page title, and selection with **zero OS permissions**, identically on every platform —
including GNOME Wayland, where nothing else works. Manifest V3, Chrome + Firefox. Channel to the
app via native messaging (correct, needs a host manifest per browser) or a 127.0.0.1 endpoint with
a token (simpler).

### M4 — screenshots + OCR → `v0.4`

Region-capture hotkey → blob on disk → capture row. OCR into `blobs.ocr_text`, FTS-indexed so
screenshots are searchable. macOS Vision first (free, excellent, no dependency); Tesseract for
Linux behind a feature flag.

### M5 — git prompt packs → `v1.0`

`magpie add github:someone/agent-prompts`, manifest at `magpie.json`. Packs are git repos — no
server, no accounts, no hosting, no moderation to run.

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
