<p align="center">
  <img src=".github/assets/readme-banner.png" alt="magpie — collects everything" width="100%" />
</p>

<p align="center">
  <a href="https://github.com/akshaykrishh/magpie/actions/workflows/ci.yml"><img src="https://github.com/akshaykrishh/magpie/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/akshaykrishh/magpie/releases/latest"><img src="https://img.shields.io/github/v/release/akshaykrishh/magpie" alt="Latest release"></a>
  <a href="https://akshaykrishh.github.io/magpie"><img src="https://img.shields.io/badge/docs-akshaykrishh.github.io%2Fmagpie-blue" alt="Documentation"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/akshaykrishh/magpie" alt="License: GPL-3.0"></a>
</p>

An open-source capture tool for AI-assisted work — a to-do list, clipboard, and scratchpad built
for people who work across ChatGPT, Claude, Cursor, and agent CLIs. Runs on macOS and Linux.
**Agents can read and write your queue over MCP.**

<!--
  TODO before release: swap this comment for a real screenshot or short GIF —
  the capture toast confirming a save, or the main window (stream + Now).
  Suggested: ~800px wide, dropped into .github/assets/, then:

  <p align="center">
    <img src=".github/assets/demo.gif" alt="magpie capturing a ChatGPT answer with one hotkey, then an agent draining it over MCP" width="800" />
  </p>
-->

**Status: early development.** The capture core, MCP server, and CLI work end-to-end on macOS.
[Signed Linux releases](https://github.com/akshaykrishh/magpie/releases) are available now
(`.deb` and `.AppImage`, in-app auto-updating) — macOS releases are pending code signing, see
[Installation](#installation) below. The full test suite runs and passes in CI on every push, but
nobody has yet run the actual desktop app on real Linux hardware by hand — see
[SECURITY.md](SECURITY.md) for verifying a download in the meantime.

## Why

Working with AI means constantly collecting small things you don't want to lose — an answer
worth keeping, a link, three follow-up prompts that occur to you while the current one is still
generating. They scatter across tabs and apps: ChatGPT, Claude, Cursor, a browser tab, a
terminal. magpie sits next to all of it, one hotkey away, and — unlike a plain clipboard manager
— lets your agents read and drain the queue directly instead of it being a dead end only human
hands can empty.

## Features

- **One hotkey, zero setup.** `Cmd/Ctrl+Shift+M` captures whatever's on the clipboard, with
  provenance (source app, and window title if Accessibility is granted) attached automatically.
  No permissions dialog on first run.
- **Screenshots, searchable.** `Cmd/Ctrl+Shift+Alt+M` opens the OS's own region picker; the
  image is OCR'd in the background and becomes full-text searchable once recognition finishes.
- **`Now`, a queue you curate.** Everything captured lands in one reverse-chronological stream —
  nothing is auto-filed. You decide what's actually actionable and promote it into `Now`, a
  short, project-scoped working queue.
- **Agents read and write the queue over MCP.** A Model Context Protocol server exposes
  `queue_peek`, `queue_take`, `capture_done`, `capture_fail`, `capture_add`, and `capture_search`
  to Claude Code, Cursor, or anything else that speaks MCP — leased, audited, never silently
  auto-expired.
- **A CLI for everything else.** `magpie add|list|search|done|drain|export|serve-mcp` —
  scriptable, and `drain` pipes `Now` straight into an agent CLI that doesn't speak MCP.
- **Prompt packs.** `magpie pack add github:someone/agent-prompts` imports shareable,
  fill-in-the-blank prompt templates from any git repo.
- **Sessions, handback, and digests.** An MCP-connected agent's session is a persistent record;
  `capture_handback` gives it a third outcome beyond done/fail, and ending a session writes a
  searchable summary into the stream.
- **Actually cross-platform.** Built on portable primitives — the OS clipboard, the freedesktop
  screenshot portal — instead of macOS-only APIs where an alternative exists. Linux is a
  first-class backend, not a port.
- **Signed, auto-updating releases.** `v0.1.0` onward ships signed, checksummed, attested Linux
  builds; the app checks for and installs updates with your explicit confirmation before anything
  runs.
- **Local-only, on purpose.** No server, no accounts, no sync, no telemetry. SQLite on disk, at a
  documented path — own your data through transparency, not lock-in.

## Quick start

**Capture from anywhere:**

- `Cmd/Ctrl+Shift+M` — capture the clipboard, with provenance attached automatically.
- `Cmd/Ctrl+Shift+Alt+M` — screenshot a region; OCR'd in the background, searchable once done.

**Work the queue from a terminal:**

```bash
magpie add "check whether the retry logic handles a 429"
magpie list --now
magpie search retry
```

**Hand it to an agent over MCP:**

```bash
claude mcp add magpie -- /path/to/magpie serve-mcp
```

```
> what's in my magpie queue for this project?
```

The agent calls `queue_peek`, sees what you promoted into `Now`, and leases it with `queue_take`
to start working. Every lease, completion, and failure lands in an audit log, visible in the
app's Activity tab.

| Tool | What it does |
|---|---|
| `queue_peek` | See what's queued in `Now` without claiming anything. |
| `queue_take` | Lease exactly one item, until `capture_done` or `capture_fail`. |
| `capture_done` / `capture_fail` | Complete or release a leased item. |
| `capture_add` | Write something back to the stream — a note, a TODO, a link. |
| `capture_search` | Full-text search over the whole stream, including unreviewed items. |

Full walkthrough: [Quick Start](https://akshaykrishh.github.io/magpie/docs/quick-start) — full
tool contract, trust tiers, and the injection-surface writeup:
[MCP integration](https://akshaykrishh.github.io/magpie/docs/mcp).

## Installation

**Linux** — signed, auto-updating. Download the `.AppImage` or `.deb` from the
[latest release](https://github.com/akshaykrishh/magpie/releases/latest):

- **`.AppImage`** — no install step, self-updates in place through the app.
- **`.deb`** — installs via `dpkg`/`apt`, does **not** self-update; reinstall by hand per release.

**macOS** — not published yet (code signing pending, see [ROADMAP.md](ROADMAP.md)). Build from
source:

```bash
git clone https://github.com/akshaykrishh/magpie.git
cd magpie/apps/desktop
pnpm install
pnpm tauri dev          # or `pnpm tauri build` for a release bundle
```

Requires [Rust](https://www.rust-lang.org/tools/install) (stable) and
[pnpm](https://pnpm.io/installation); macOS also needs Xcode Command Line Tools. This is a Cargo
workspace, so the CLI and MCP server binaries build the same way:

```bash
cargo build --release -p magpie-cli   # binary at target/release/magpie
```

**Verify a download:**

```bash
gh release download vX.Y.Z --repo akshaykrishh/magpie
sha256sum -c SHA256SUMS
gh attestation verify magpie_X.Y.Z_amd64.AppImage --repo akshaykrishh/magpie
```

Full instructions, unsigned CI builds, and what verification does and doesn't prove:
[Installation](https://akshaykrishh.github.io/magpie/docs/installation) ·
[SECURITY.md](SECURITY.md).

## Documentation

Full docs: **[akshaykrishh.github.io/magpie](https://akshaykrishh.github.io/magpie)**

| | |
|---|---|
| [Quick Start](https://akshaykrishh.github.io/magpie/docs/quick-start) | First capture to first agent hand-off |
| [Installation](https://akshaykrishh.github.io/magpie/docs/installation) | Downloads, verification, building from source |
| [CLI reference](https://akshaykrishh.github.io/magpie/docs/cli) | Every `magpie` subcommand |
| [MCP integration](https://akshaykrishh.github.io/magpie/docs/mcp) | Registering with Claude Code/Cursor, the tool contract, trust tiers |
| [Architecture](https://akshaykrishh.github.io/magpie/docs/architecture) | How it's built, what's verified vs. implemented |
| [Concepts](https://akshaykrishh.github.io/magpie/docs/concepts) | Stream & Now, Projects, Screenshots & OCR, Templates & packs |
| [Schema](https://akshaykrishh.github.io/magpie/docs/schema) | The SQLite schema on disk |

[docs/design.md](docs/design.md) is the original design record — the full reasoning behind each
architectural decision, kept separate from the docs site because it's a record, not a manual.

## Roadmap

The capture core, MCP server + CLI, and prompt packs are done and usable today. Linux releases
are signed and auto-update; macOS code signing (needed before a Homebrew cask) is still manual.
**[ROADMAP.md](ROADMAP.md)** has what's next — in Now/Next/Later form — and what this project is
deliberately not doing. See **[CHANGELOG.md](CHANGELOG.md)** for release history.

## Contributing

Push access is held by the maintainer alone; open a PR and it'll be reviewed. See
**[CONTRIBUTING.md](CONTRIBUTING.md)** for dev setup and what CI checks before a PR. This project
follows a **[Code of Conduct](CODE_OF_CONDUCT.md)**.

## Security

Local-only app: no server, no accounts, no telemetry. See **[SECURITY.md](SECURITY.md)** for the
update signing/trust chain, how to verify a download, and how to report a vulnerability.

## License

GPL-3.0. See [LICENSE](LICENSE). The `magpie` name and any project logo are not covered by the
license and are reserved.
