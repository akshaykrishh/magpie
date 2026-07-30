# magpie

An open-source capture tool for AI-assisted work — a to-do list, clipboard, and scratchpad built
for people who work across ChatGPT, Claude, Cursor, and agent CLIs. Runs on macOS and Linux.
Agents can read and write your queue over MCP.

`magpie` is a working name, not final.

**Status: pre-alpha.** Nothing here is usable yet. See [the design document](docs/design.md) for
the full architecture and roadmap.

## Why

Working with AI means constantly collecting small things you don't want to lose — an answer
worth keeping, a link, three follow-up prompts that occur to you while the current one is still
generating. They scatter across tabs and apps: ChatGPT, Claude, Cursor, a browser tab, a
terminal. magpie sits next to all of it, one hotkey away, and — unlike a plain clipboard manager
— lets your agents read and drain the queue directly instead of it being a dead end only human
hands can empty.

## Roadmap

Building toward a first real release (`v0.1`) in stages, each one a working, installable app:

- **Capture core** — the stream, the `Now` working set, search, merge, tags, projects
- **MCP server + CLI** — agents can read and write the queue over the Model Context Protocol
- **Browser extension** — exact URL/page provenance with zero OS permissions
- **Screenshots + OCR**
- **Prompt templates and packs**

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
