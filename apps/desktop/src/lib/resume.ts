import { api } from "./api";
import type { ProjectFilter } from "./types";
import type { ResumeContext } from "@/components/ResumeCard";

/// Assembles one project row's `ResumeContext` purely from existing IPC --
/// no dedicated backend query needed, since `list_stream_rows` already
/// returns session digests alongside ordinary captures (see
/// `Capture.kind`) and `list_now` already carries `handback_at`/`done_at`.
/// Fetches a handful of recent rows (5) rather than the whole stream:
/// enough to find the latest capture and a couple of recent digests
/// without pulling a project's entire history for what's meant to be a
/// glance, not a browse.
export async function fetchResumeContext(projectId: number | null): Promise<ResumeContext> {
  const filter: ProjectFilter = projectId === null ? { kind: "inbox" } : { kind: "project", id: projectId };
  const [rows, nowItems] = await Promise.all([
    api.listStreamRows(filter, 5, 0),
    api.listNow(projectId),
  ]);
  return {
    lastCapture: rows.find((r) => r.capture.kind !== "session_digest") ?? null,
    digests: rows.filter((r) => r.capture.kind === "session_digest"),
    unreviewed: nowItems.filter((c) => c.handback_at !== null && c.done_at === null),
  };
}
