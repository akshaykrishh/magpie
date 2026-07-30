import { formatDistanceToNow } from "date-fns";
import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { AuditEntry } from "@/lib/types";

const ACTION_LABELS: Record<string, string> = {
  queue_take: "took",
  capture_done: "completed",
  capture_fail: "failed",
  session_disconnected_released_leases: "disconnected (released leases)",
  lease_released_dead_process: "lease recovered (process died)",
};

/// What an agent did while you were away -- see docs/design.md "Agent
/// trust": this is what turns that from a mystery into a scroll.
export function AuditView() {
  const [entries, setEntries] = useState<AuditEntry[]>([]);

  useEffect(() => {
    const refresh = () => api.listAudit(100).then(setEntries).catch(console.error);
    refresh();
    const interval = setInterval(refresh, 3000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="flex h-full flex-col gap-3 overflow-y-auto p-3">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-400">
        Agent activity
      </h2>
      {entries.length === 0 ? (
        <p className="px-1 text-sm text-neutral-400 dark:text-neutral-600">
          Nothing yet. Every action an MCP client takes on your queue shows up here.
        </p>
      ) : (
        <div className="flex flex-col gap-1">
          {entries.map((e) => (
            <div
              key={e.id}
              className="flex items-baseline gap-2 rounded-md px-2 py-1 text-sm hover:bg-neutral-50 dark:hover:bg-neutral-900"
            >
              <span className="font-medium text-neutral-700 dark:text-neutral-300">
                {e.actor}
              </span>
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
      )}
    </div>
  );
}
