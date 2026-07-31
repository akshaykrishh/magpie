import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import type { Capture, Template } from "@/lib/types";

export function RecentlyDeletedView() {
  const [captures, setCaptures] = useState<Capture[]>([]);
  const [templates, setTemplates] = useState<Template[]>([]);

  // Guards against out-of-order responses: each refresh() bumps this and
  // captures the new value locally, so a slower, earlier-fired request that
  // resolves after a newer one can detect it's stale (the ref will have
  // moved on) and discard its result instead of clobbering fresher state.
  // Needed because refresh() can be in flight more than once at a time --
  // e.g. two quick restores of different rows, or a double-clicked Restore
  // button (there's no disabled/loading state on it) -- and nothing else
  // sequences those calls relative to each other.
  const refreshGeneration = useRef(0);

  function refresh() {
    const generation = ++refreshGeneration.current;
    api
      .listRecentlyDeletedCaptures()
      .then((result) => {
        if (refreshGeneration.current === generation) setCaptures(result);
      })
      .catch(console.error);
    api
      .listRecentlyDeletedTemplates()
      .then((result) => {
        if (refreshGeneration.current === generation) setTemplates(result);
      })
      .catch(console.error);
  }

  useEffect(refresh, []);

  async function restoreCapture(id: number) {
    try {
      await api.restoreCapture(id);
      refresh();
    } catch (err) {
      console.error(err);
    }
  }

  async function restoreTemplate(id: number) {
    try {
      await api.restoreTemplate(id);
      refresh();
    } catch (err) {
      console.error(err);
    }
  }

  // No confirmation dialog, consistent with the rest of the app (soft-delete
  // + Undo replaces confirm() everywhere else, and this codebase has none).
  // Permanent delete is the one place here with no further undo, but it's
  // already reached through two deliberate steps -- the item was already
  // soft-deleted once, and this view is a dedicated destination for exactly
  // this action -- rather than a single misclick away from data loss the way
  // an unprotected top-level delete button would be. The button itself is
  // styled distinctly (red, separated from Restore) so it doesn't read as a
  // second Restore action at a glance.
  async function deleteCapturePermanently(id: number) {
    try {
      await api.deleteCapturePermanently(id);
      refresh();
    } catch (err) {
      console.error(err);
    }
  }

  async function deleteTemplatePermanently(id: number) {
    try {
      await api.deleteTemplatePermanently(id);
      refresh();
    } catch (err) {
      console.error(err);
    }
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
                <div className="flex shrink-0 items-center gap-3">
                  <button
                    type="button"
                    onClick={() => restoreCapture(c.id)}
                    className="text-slate-teal hover:underline dark:text-slate-teal-light"
                  >
                    Restore
                  </button>
                  <button
                    type="button"
                    onClick={() => deleteCapturePermanently(c.id)}
                    className="text-red-500 hover:underline dark:text-red-400"
                  >
                    Delete Permanently
                  </button>
                </div>
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
                <div className="flex shrink-0 items-center gap-3">
                  <button
                    type="button"
                    onClick={() => restoreTemplate(t.id)}
                    className="text-slate-teal hover:underline dark:text-slate-teal-light"
                  >
                    Restore
                  </button>
                  <button
                    type="button"
                    onClick={() => deleteTemplatePermanently(t.id)}
                    className="text-red-500 hover:underline dark:text-red-400"
                  >
                    Delete Permanently
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
