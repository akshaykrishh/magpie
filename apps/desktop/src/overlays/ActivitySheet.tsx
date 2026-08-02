import { formatDistanceToNow } from "date-fns";
import { useEffect, useState } from "react";
import { Mono } from "@/components/ui";
import { api } from "@/lib/api";
import { sessionLabel } from "@/lib/format";
import type { AuditEntryView } from "@/lib/types";

const ACTION_LABELS: Record<string, string> = {
  queue_take: "took",
  capture_done: "completed",
  capture_fail: "failed",
  capture_handback: "handed back",
  session_disconnected_released_leases: "disconnected (released leases)",
  lease_released_dead_process: "lease recovered (process died)",
  lease_revoked_by_human: "lease revoked",
};

interface SessionGroup {
  key: string;
  ordinal: number | null;
  client: string | null;
  branch: string | null;
  live: boolean | null;
  entries: AuditEntryView[];
}

const EARLIER_KEY = "__earlier__";

// Groups by `session_id`, not by time -- see the redesign plan and
// migrations/0010_ui_provenance.sql. Rows written before that migration
// (or any row whose action genuinely has no session, like a plain human
// GUI action once those are audited) have `session_id: null` and land in
// one "Earlier" bucket rather than being guessed into a session they may
// not belong to.
function groupBySession(entries: AuditEntryView[]): SessionGroup[] {
  const groups = new Map<string, SessionGroup>();
  const order: string[] = [];
  for (const e of entries) {
    const key = e.session_id ?? EARLIER_KEY;
    if (!groups.has(key)) {
      groups.set(key, {
        key,
        ordinal: e.session_ordinal,
        client: e.session_client,
        branch: e.session_branch,
        live: e.session_live,
        entries: [],
      });
      order.push(key);
    }
    groups.get(key)!.entries.push(e);
  }
  // Live sessions first, then ended ones, "Earlier" always last -- ties
  // otherwise keep the query's own `at DESC` ordering (first-seen order),
  // since Array.prototype.sort is stable.
  return order
    .map((k) => groups.get(k)!)
    .sort((a, b) => {
      if (a.key === EARLIER_KEY) return 1;
      if (b.key === EARLIER_KEY) return -1;
      if (a.live !== b.live) return a.live ? -1 : 1;
      return 0;
    });
}

/// What an agent did while you were away -- see docs/design.md "Agent
/// trust": this is what turns that from a mystery into a scroll. Grouped
/// by session (see groupBySession) rather than AuditView's old flat,
/// polled-every-3s list -- opened fresh each time via ⌘L instead, since a
/// sheet that's about to be dismissed doesn't need to keep polling behind
/// the scenes.
export function ActivitySheet() {
  const [entries, setEntries] = useState<AuditEntryView[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    api
      .listAuditEnriched(200)
      .then((e) => {
        setEntries(e);
        setLoaded(true);
      })
      .catch(console.error);
  }, []);

  const groups = groupBySession(entries);

  return (
    <div className="flex h-full flex-col gap-4 overflow-y-auto p-4">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-400">
        Agent activity
      </h2>

      {loaded && entries.length === 0 && (
        <p className="px-1 text-sm text-neutral-400 dark:text-neutral-600">
          Nothing yet. Every action an MCP client takes on your queue shows up here.
        </p>
      )}

      <div className="flex flex-col gap-4">
        {groups.map((g) => (
          <div key={g.key}>
            <div className="mb-1.5 flex items-center gap-2">
              <Mono size="sm" tone={g.live ? "accent" : "faint"}>
                {g.key === EARLIER_KEY
                  ? "EARLIER"
                  : sessionLabel({ ordinal: g.ordinal, client: g.client })}
              </Mono>
              {g.branch && (
                <Mono size="xs" tone="faint">
                  {g.branch}
                </Mono>
              )}
              {g.live && (
                <Mono size="xs" tone="accent">
                  LIVE
                </Mono>
              )}
            </div>
            <div className="flex flex-col gap-1">
              {g.entries.map((e) => (
                <div
                  key={e.id}
                  className="flex items-baseline gap-2 rounded-md px-2 py-1 text-sm hover:bg-neutral-50 dark:hover:bg-neutral-900"
                >
                  <span className="text-neutral-400 dark:text-neutral-600">{e.actor}</span>
                  <span className="text-neutral-500 dark:text-neutral-400">
                    {ACTION_LABELS[e.action] ?? e.action}
                  </span>
                  {e.capture_id !== null && (
                    <span className="text-neutral-400 dark:text-neutral-600">
                      #{e.capture_id}
                    </span>
                  )}
                  <span className="ml-auto shrink-0 text-xs text-neutral-400 dark:text-neutral-600">
                    {formatDistanceToNow(new Date(e.at), { addSuffix: true })}
                  </span>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
