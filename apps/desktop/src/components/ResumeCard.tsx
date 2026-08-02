import { elapsed } from "@/lib/format";
import { Chip, Mono } from "./ui";
import type { Capture, StreamRow } from "@/lib/types";

export interface ResumeContext {
  lastCapture: StreamRow | null;
  /// Most recent session digests first, already filtered to this
  /// project's `session_digest` rows by the caller (see
  /// `lib/resume.ts`'s `fetchResumeContext`) rather than a dedicated
  /// backend query -- `list_stream_rows` already returns them.
  digests: StreamRow[];
  /// Now items with `handback_at` set and `done_at` still null -- the same
  /// `needs_review()` predicate `list_projects_overview`'s own count uses
  /// (crates/magpie-core/src/model.rs), just returning the rows instead of
  /// a count.
  unreviewed: Capture[];
}

/// "What needs a look" for one project row in Across -- purely a
/// reassembly of data that already exists (last capture, session digests,
/// unreviewed hand-backs). No LLM call, no summarization: don't add one
/// later without changing this comment first, since that would be a
/// different, much more expensive feature than what Across ships with.
export function ResumeCard({ lastCapture, digests, unreviewed }: ResumeContext) {
  const hasAnything = lastCapture || digests.length > 0 || unreviewed.length > 0;
  if (!hasAnything) {
    return (
      <p className="px-3 py-6 text-center text-body-sm text-fg-faint">Nothing to resume here yet.</p>
    );
  }

  return (
    <div className="flex flex-col gap-3 px-3 py-3">
      {unreviewed.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <Mono size="xs" tone="accent">
            {unreviewed.length} waiting review
          </Mono>
          {unreviewed.slice(0, 2).map((c) => (
            <p key={c.id} className="truncate text-body-sm text-fg">
              {c.handback_note || c.body}
            </p>
          ))}
        </div>
      )}

      {lastCapture && (
        <div className="flex flex-col gap-1">
          <Mono size="xs" tone="faint">
            last capture · {elapsed(lastCapture.capture.created_at)}
          </Mono>
          <p className="truncate text-body-sm text-fg-muted">{lastCapture.capture.body}</p>
        </div>
      )}

      {digests.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <Mono size="xs" tone="faint">
            session digests
          </Mono>
          {digests.slice(0, 2).map((d) => (
            <p key={d.capture.id} className="line-clamp-2 text-body-sm text-fg-muted">
              {d.capture.body}
            </p>
          ))}
        </div>
      )}

      {lastCapture?.capture.branch && (
        <Chip variant="neutral" className="self-start">
          ON {lastCapture.capture.branch.toUpperCase()}
        </Chip>
      )}
    </div>
  );
}
