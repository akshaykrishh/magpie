import { formatDistanceToNow } from "date-fns";
import { useCallback, useEffect, useState } from "react";
import { Mono } from "../components/ui";
import { api } from "../lib/api";
import type { AuditEntryView, Session } from "../lib/types";

// Kept in sync with ActivitySheet.tsx's ACTION_LABELS by hand (see Task 5
// Step 1) -- not imported from there, since ActivitySheet.tsx doesn't
// currently export it and this view's row shape is intentionally
// simpler (no session grouping, just a flat recent list).
const ACTION_LABELS: Record<string, string> = {
  queue_take: "took",
  capture_done: "completed",
  capture_fail: "failed",
  capture_handback: "handed back",
  session_disconnected_released_leases: "disconnected (released leases)",
  lease_released_dead_process: "lease recovered (process died)",
  lease_revoked_by_human: "lease revoked",
};

const RECENT_LIMIT = 15;
const FETCH_LIMIT = 200;

interface DockActivityViewProps {
  projectId: number | null;
  sessions: Session[];
}

/// A compact, ambient version of the main window's Activity Sheet (⌘L) --
/// the last ~15 actions for whichever project is currently focused or
/// peeked, not the full cross-project log. list_audit_enriched has no
/// project filter server-side, so this joins against `sessions` (already
/// fetched by DockApp's own poll) client-side instead of adding one.
export function DockActivityView({ projectId, sessions }: DockActivityViewProps) {
  const [entries, setEntries] = useState<AuditEntryView[]>([]);

  const refresh = useCallback(() => {
    api.listAuditEnriched(FETCH_LIMIT).then(setEntries).catch(console.error);
  }, []);

  useEffect(() => {
    refresh();
    const poll = setInterval(refresh, 5000);
    return () => clearInterval(poll);
  }, [refresh]);

  const sessionProjectIds = new Map(sessions.map((s) => [s.id, s.project_id]));
  const scoped = entries
    .filter((e) => e.session_id !== null && sessionProjectIds.get(e.session_id) === projectId)
    .slice(0, RECENT_LIMIT);

  return (
    <div className="flex-1 overflow-y-auto">
      {scoped.length === 0 && (
        <p className="px-1 py-4 text-center text-body-sm text-fg-faint">
          Nothing yet for this project.
        </p>
      )}
      <div className="flex flex-col gap-1">
        {scoped.map((e) => (
          <div key={e.id} className="flex items-baseline gap-1.5 px-1 py-1 text-body-sm">
            <span className="text-fg-faint">{e.actor}</span>
            <span className="text-fg-muted">{ACTION_LABELS[e.action] ?? e.action}</span>
            <Mono size="xs" tone="faint" className="ml-auto shrink-0">
              {formatDistanceToNow(new Date(e.at), { addSuffix: true })}
            </Mono>
          </div>
        ))}
      </div>
    </div>
  );
}
