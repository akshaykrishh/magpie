<img src=".github/assets/readme-banner.png" alt="magpie" width="100%" />

An open-source capture tool for AI-assisted work — a to-do list, clipboard, and scratchpad built
for people who work across ChatGPT, Claude, Cursor, and agent CLIs. Runs on macOS and Linux.
Agents can read and write your queue over MCP.

**Status: early development.** The capture core, MCP server, and CLI work end-to-end on macOS.
On Linux, the full test suite runs and passes in CI on every push, but nobody has yet run the
actual desktop app on a real Linux machine — that first-hand verification is still outstanding.
No signed releases yet; CI produces unsigned build artifacts on every push (see Development
below to build from source in the meantime).

**Docs: [akshaykrishh.github.io/magpie](https://akshaykrishh.github.io/magpie)** -- installation,
concepts, the CLI and MCP reference, architecture, and the schema. [docs/design.md](docs/design.md)
remains the original design record with the full reasoning behind each decision.

## Why

Working with AI means constantly collecting small things you don't want to lose — an answer
worth keeping, a link, three follow-up prompts that occur to you while the current one is still
generating. They scatter across tabs and apps: ChatGPT, Claude, Cursor, a browser tab, a
terminal. magpie sits next to all of it, one hotkey away, and — unlike a plain clipboard manager
— lets your agents read and drain the queue directly instead of it being a dead end only human
hands can empty.

## Roadmap

The capture core, MCP server + CLI, and prompt packs are done and usable today. Screenshots + OCR
work on macOS; Linux has the code but not yet a first-hand run on real hardware. Packaging ships
unsigned installers from CI; real code signing is still a manual step.

**[ROADMAP.md](ROADMAP.md)** has what's next, in Now/Next/Later form, and what this project is
deliberately not doing. **[docs/design.md](docs/design.md)** is the architecture and the reasoning
behind each decision.

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
