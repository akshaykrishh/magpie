import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { Capture, Template } from "@/lib/types";

export function RecentlyDeletedView() {
  const [captures, setCaptures] = useState<Capture[]>([]);
  const [templates, setTemplates] = useState<Template[]>([]);

  function refresh() {
    api.listRecentlyDeletedCaptures().then(setCaptures).catch(console.error);
    api.listRecentlyDeletedTemplates().then(setTemplates).catch(console.error);
  }

  useEffect(refresh, []);

  async function restoreCapture(id: number) {
    await api.restoreCapture(id);
    refresh();
  }

  async function restoreTemplate(id: number) {
    await api.restoreTemplate(id);
    refresh();
  }

  if (captures.length === 0 && templates.length === 0) {
    return (
      <p className="px-3 py-6 text-center text-sm text-neutral-400 dark:text-neutral-600">
        Nothing recently deleted.
      </p>
    );
  }

  return (
    <div className="flex flex-col gap-4 overflow-y-auto p-3">
      {captures.length > 0 && (
        <div>
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-neutral-400">
            Captures
          </h3>
          <div className="flex flex-col gap-2">
            {captures.map((c) => (
              <div
                key={c.id}
                className="flex items-center justify-between rounded-lg border
                           border-neutral-200 bg-white px-3 py-2 text-sm dark:border-neutral-800
                           dark:bg-neutral-900"
              >
                <span className="truncate text-neutral-700 dark:text-neutral-300">
                  {c.body || "(screenshot)"}
                </span>
                <button
                  type="button"
                  onClick={() => restoreCapture(c.id)}
                  className="shrink-0 text-slate-teal hover:underline dark:text-slate-teal-light"
                >
                  Restore
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
      {templates.length > 0 && (
        <div>
          <h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-neutral-400">
            Templates
          </h3>
          <div className="flex flex-col gap-2">
            {templates.map((t) => (
              <div
                key={t.id}
                className="flex items-center justify-between rounded-lg border
                           border-neutral-200 bg-white px-3 py-2 text-sm dark:border-neutral-800
                           dark:bg-neutral-900"
              >
                <span className="truncate text-neutral-700 dark:text-neutral-300">
                  {t.title}
                </span>
                <button
                  type="button"
                  onClick={() => restoreTemplate(t.id)}
                  className="shrink-0 text-slate-teal hover:underline dark:text-slate-teal-light"
                >
                  Restore
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
