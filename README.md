<img src=".github/assets/readme-banner.png" alt="magpie" width="100%" />

An open-source capture tool for AI-assisted work — a to-do list, clipboard, and scratchpad built
for people who work across ChatGPT, Claude, Cursor, and agent CLIs. Runs on macOS and Linux.
Agents can read and write your queue over MCP.

**Status: early development.** The capture core, MCP server, and CLI work end-to-end on macOS.
On Linux, the full test suite runs and passes in CI on every push, but nobody has yet run the
actual desktop app on a real Linux machine — that first-hand verification is still outstanding.
No signed releases yet; CI produces unsigned build artifacts on every push (see Development
below to build from source in the meantime). See [the design document](docs/design.md) for the
full architecture and roadmap.

## Why

Working with AI means constantly collecting small things you don't want to lose — an answer
worth keeping, a link, three follow-up prompts that occur to you while the current one is still
generating. They scatter across tabs and apps: ChatGPT, Claude, Cursor, a browser tab, a
terminal. magpie sits next to all of it, one hotkey away, and — unlike a plain clipboard manager
— lets your agents read and drain the queue directly instead of it being a dead end only human
hands can empty.

## Roadmap

Building toward a first packaged release in stages:

- **Capture core** — the stream, the `Now` working set, search, merge, tags, projects (done)
- **MCP server + CLI** — agents can read and write the queue over the Model Context Protocol,
  reusable prompt templates (done)
- **Screenshots + OCR** — region capture, searchable via on-device text recognition (done on
  macOS; implemented on Linux but not yet verified on real hardware)
- **Prompt packs** — shared, git-hosted collections of templates, with fill-in-the-blank prompts
  (done)
- **Packaging** — CI builds unsigned installers for macOS and Linux on every push (done); real
  code signing and notarization are still a manual step (see docs/design.md "Signing")

See [docs/design.md](docs/design.md) for the full architecture, the reasoning behind each design
decision, and the current status of each piece.

## License

GPL-3.0. See [LICENSE](LICENSE). The `magpie` name and any project logo are not covered by the
license and are reserved.

## Development

```sh
cd apps/desktop
pnpm install
pnpm tauri dev
```

Requires Rust (stable) and pnpm. macOS builds also require Xcode Command Line Tools.
