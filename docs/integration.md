# Working with starling

magpie collects the work you want agents to do and hands it to them;
starling keeps the sessions doing that work running
safely. Each is complete on its own. When both are installed they find each other with no
configuration and each shows the other's half.

## What lights up

| Where | With magpie installed |
|---|---|
| Session start | Under the stream's handoff: the magpie items queued for this repo and branch |
| `starling ls` | A QUEUE column (open items for each session's repo and branch) and, for a session whose agent has taken an item, that item's title |
| `starling handoff` | The generated state lists the open magpie items |
| `starling close` | Warns when magpie still has open items for the stream's branch |

Nothing to set up on the magpie side: starling calls the two read-only commands below.

And with starling installed, the magpie desktop app shows:

| Where | With starling installed |
|---|---|
| Dock | An agent row for each Claude Code session working on the focused project: "Needs you" when starling says it is waiting, and sessions that have not called a magpie tool yet appear too, marked "not connected to magpie" |
| Across | Those starling-only sessions count as live in a project's agent count, and every waiting agent counts as "needs you" |
| Session strip | A "needs you" chip on a session whose agent is waiting |
| Hold-to-aim | Each open stream whose folder is a magpie project is listed under that project; aiming at one files the capture to the project and, when the stream is not on the default branch, pins it to that branch |

Nothing to set up here either: the app calls the two read-only commands in the next section.
It looks for `starling` (`STARLING_BIN`, then `PATH`, then your login shell, then Homebrew and
nvm locations) because an app opened from the Dock does not inherit your shell's `PATH`. If it
finds nothing, none of this appears.

## The contract (schema 1)

Each side only **reads** the other's JSON output. A missing binary, a non-zero exit, output that
is not JSON, or a `schema` other than 1 all mean "not installed": the feature is silently
absent, never an error. Calls time out after 2 seconds.

### starling provides

`starling ls --json [--all] [--hours N]`

```json
{ "schema": 1,
  "sessions": [
    { "id": "…", "profile": "work", "cwd": "/abs/path", "branch": "refunds",
      "title": "Partial refunds", "state": "waiting", "since": 1730000000000,
      "updated": 1730000000000, "pid": 4242, "stream": "billing-retry" } ] }
```

`state` is one of `working`, `waiting`, `idle`, `active`, `ended`. `pid` is the `claude`
process (absent when unknown). Times are epoch milliseconds.

`starling streams --json`

```json
{ "schema": 1,
  "streams": [
    { "name": "billing-retry", "cwd": "/abs/path", "profile": "work", "status": "open",
      "created": 1730000000000, "updated": 1730000000000 } ] }
```

### magpie provides

`magpie now --json [--project <dir>] [--branch <name>]`

```json
{ "schema": 1,
  "project": { "id": 3, "name": "shop-api" },
  "items": [
    { "id": 41, "title": "Retry webhook should cover partial refunds", "branch": null,
      "position": 1, "lease": { "session": "…", "pid": 5151, "at": "2026-10-21T10:00:00Z" } } ] }
```

- `--project <dir>` resolves the folder to a magpie project exactly as the MCP server does
  for its spawn directory (git remote URL, else the common git dir). An unknown project gives
  `"project": null, "items": []`. Without `--project` the command reads the Inbox, the same
  scope as `magpie list --now`, and `project` is null.
- `--branch <name>` keeps items whose `branch` is null or equal to `name`.
- `title` is the first line of the item; `lease` is null when nobody has taken it.
- `position` is the item's 1-based rank in the project's whole Now queue; filtering with
  `--branch` does not renumber it.

`magpie sessions --json [--live]`

```json
{ "schema": 1,
  "sessions": [
    { "id": "…", "client": "claude-code", "pid": 5151,
      "project": { "id": 3, "name": "shop-api" }, "branch": "refunds",
      "started_at": "2026-10-21T09:58:00Z", "ended_at": null,
      "leased": { "id": 41, "title": "Retry webhook should cover partial refunds" } } ] }
```

`--live` keeps sessions with `ended_at` null. `pid` is magpie's MCP server process. Sessions
are listed newest first.

### Matching sessions

magpie's MCP server is a child of the `claude` process. starling records the `claude` pid, so a
magpie session belongs to the starling session whose pid is the parent of magpie's `pid`
(`ps -o ppid= -p <pid>`). Same machine only; no ids are exchanged.

## Notes on reading

Both commands only read. They do not run the dead-pid sweep, release or take leases, or create
a project or a database: with no database yet, `now` reports `"project": null, "items": []`
and `sessions` reports `[]`, with exit 0. A database written by an older magpie is migrated on
first read, the same as every other command does. Fields that magpie has not recorded (a
session's `client` before its first tool call, a lease's `pid`) are `null`. Timestamps are UTC
RFC 3339 with sub-second precision, for example `2026-10-21T10:00:00.123456000Z`.
