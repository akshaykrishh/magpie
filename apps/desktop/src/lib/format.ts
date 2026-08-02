/// Elapsed time since an ISO timestamp, in the design's compact form:
/// `12M`, `1H12M`, `2D`. Deliberately elapsed, never a countdown -- leases
/// have no expiry by design (see docs/design.md and the redesign plan's
/// "leases keep no expiry" decision), so every duration this app shows is
/// "how long has this been true," never "how long until." `LEASED BY S1 ·
/// 12M` reads correctly under that rule; a mockup's `12M LEFT` would not.
export function elapsed(iso: string, now: Date = new Date()): string {
  const then = new Date(iso).getTime();
  const ms = Math.max(0, now.getTime() - then);
  const minutes = Math.floor(ms / 60_000);
  if (minutes < 60) return `${minutes}M`;
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  if (hours < 24) return remainingMinutes > 0 ? `${hours}H${remainingMinutes}M` : `${hours}H`;
  const days = Math.floor(hours / 24);
  return `${days}D`;
}

/// `S1 · CLAUDE CODE`. `client` is NULL until the agent's first tool call
/// (see magpie-core's sessions.rs) -- render `CONNECTING` in that window
/// rather than guessing a name, per the redesign plan's "render as
/// unknown, never invent" list. `ordinal` is likewise nullable on rows
/// written before migration 0010 (stage 4) adds the column; falls back to
/// `S?` rather than crashing or fabricating a number.
///
/// Takes a loose `{ ordinal, client }` shape rather than `Session` itself:
/// `ordinal` doesn't land on the `Session` wire type until stage 4, and
/// stage 7's synthesized S0 (see the redesign plan) is never a real
/// `Session` row at all, so this needs to work for both.
export function sessionLabel(session: {
  ordinal: number | null;
  client: string | null;
}): string {
  const label = session.ordinal != null ? `S${session.ordinal}` : "S?";
  const clientPart = session.client ? session.client.toUpperCase() : "CONNECTING";
  return `${label} · ${clientPart}`;
}

export type NowRowState = "open" | "leased" | "pinned" | "handback" | "done";

/// Which of the Now column's five row states a capture is in -- one glyph,
/// so a priority order is needed since the underlying columns are
/// independent axes (a leased item could theoretically also carry a
/// `branch`, an already-done item's lease columns are always null by then,
/// etc.). Priority, most to least visually loud: done (settled) >
/// handback (needs you) > leased (someone has it) > pinned (branch-
/// scoped, dimmed) > open (default). Matches `StatusGlyph`'s own doc
/// comment on why handback is the loudest of the five.
export function nowRowState(capture: {
  done_at: string | null;
  handback_at: string | null;
  lease_session: string | null;
  branch: string | null;
}): NowRowState {
  if (capture.done_at !== null) return "done";
  if (capture.handback_at !== null) return "handback";
  if (capture.lease_session !== null) return "leased";
  if (capture.branch !== null) return "pinned";
  return "open";
}

export interface DiffStatSummary {
  filesChanged: number;
  insertions: number;
  deletions: number;
  /// Individual "path | N +++---" lines, in the order git printed them.
  /// Empty when only the summary line could be parsed (or nothing could).
  files: { path: string; churn: string }[];
}

// Matches git's `--stat` summary line, e.g.:
//   " 3 files changed, 64 insertions(+), 11 deletions(-)"
//   " 1 file changed, 2 insertions(+)"
const SUMMARY_RE =
  /(\d+) files? changed(?:, (\d+) insertions?\(\+\))?(?:, (\d+) deletions?\(-\))?/;

// Matches one per-file line, e.g.:
//   " src/lib.rs | 12 +++++++++----"
//   " Cargo.toml |  2 ++"
const FILE_LINE_RE = /^\s*(.+?)\s+\|\s+(.+)$/;

/// Parses `git diff --stat` output (magpie-mcp's `project::diff_stat` --
/// see crates/magpie-mcp/src/project.rs) into a structured summary for the
/// hand-back review sheet's `+64 -11` chip and file list. This is git's
/// human-readable text output, not a machine format -- parsing is
/// defensive by design: an unparseable summary line still returns
/// whatever file lines *did* match, and a completely unparseable stat
/// (git output ever changes format, or `diff_stat` returned something
/// unexpected) returns zeros and an empty file list rather than throwing,
/// so a render never crashes on a hand-back it can't fully decode -- the
/// caller falls back to showing the raw text (see the redesign plan's
/// "+64 −11" risk note).
export function parseDiffStat(text: string | null | undefined): DiffStatSummary | null {
  if (!text) return null;
  const lines = text.split("\n");
  const summaryLine = lines.find((l) => SUMMARY_RE.test(l));
  const summaryMatch = summaryLine ? SUMMARY_RE.exec(summaryLine) : null;

  const files = lines
    .map((l) => FILE_LINE_RE.exec(l))
    .filter((m): m is RegExpExecArray => m !== null)
    .map((m) => ({ path: m[1].trim(), churn: m[2].trim() }));

  if (!summaryMatch && files.length === 0) return null;

  return {
    filesChanged: summaryMatch ? Number(summaryMatch[1]) : files.length,
    insertions: summaryMatch?.[2] ? Number(summaryMatch[2]) : 0,
    deletions: summaryMatch?.[3] ? Number(summaryMatch[3]) : 0,
    files,
  };
}
