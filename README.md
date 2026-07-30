# magpie

An open-source capture tool for AI-assisted work — a to-do list, clipboard, and scratchpad built
for people who work across ChatGPT, Claude, Cursor, and agent CLIs. Runs on macOS and Linux.
Agents can read and write your queue over MCP.

`magpie` is a working name, not final.

**Status: pre-alpha.** Nothing here is usable yet. See [the design spec](docs/design.md) for the
full plan.

## Why

Working with AI means constantly collecting small things you don't want to lose — an answer
worth keeping, a link, three follow-up prompts that occur to you while the current one is still
generating. They scatter across tabs and apps. Existing tools in this space tend to be closed,
Mac-only, and not integrated with anything. This project targets the same problem, but open
source, cross-platform, and with an MCP server so Claude Code / Cursor agents can work the queue
directly instead of it being a dead end only human hands can empty.

## Current milestone: M0

Proving the core interaction risk before building anything real on top of it: can a global
hotkey show a toast **without stealing OS keyboard focus** from whatever app you're typing in?
See `apps/desktop` — currently a throwaway harness for exactly that question, not the real app.

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
