import { useEffect, useState } from "react";
import type { Capture } from "@/lib/types";
import { MarkdownBody } from "./MarkdownBody";

interface EditableBodyProps {
  capture: Capture;
  editing: boolean;
  onSave: (body: string) => Promise<void>;
  onCancel: () => void;
}

export function EditableBody({ capture, editing, onSave, onCancel }: EditableBodyProps) {
  const [draft, setDraft] = useState(capture.body);

  // `draft` otherwise only reflects whatever `capture.body` was at mount --
  // fine for a short-lived editor, but this component can now sit mounted
  // for a long time (the row it belongs to gets re-rendered, not remounted,
  // by every unrelated refreshStream()/refreshNow() this task wired up --
  // Mark Done, Merge, Move-to, Delete, etc. all refetch without touching
  // this row's identity). Without this resync, opening the editor could
  // silently start from a stale snapshot and save over a newer body that
  // arrived from elsewhere in the meantime. Resyncing only on the
  // false-to-true transition (not on every render) is deliberate -- it
  // must NOT clobber in-progress keystrokes on every parent re-render while
  // already editing.
  useEffect(() => {
    if (editing) setDraft(capture.body);
    // Deliberately NOT depending on capture.body -- a body change that
    // arrives while already editing must not clobber in-progress keystrokes.
  }, [editing, capture.id]);

  if (!editing) {
    return <MarkdownBody text={capture.body} className="text-sm text-neutral-800 dark:text-neutral-200" />;
  }

  return (
    <div className="flex flex-col gap-2">
      <textarea
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        autoFocus
        rows={4}
        className="w-full rounded-md border border-neutral-300 bg-white p-2 text-sm
                   dark:border-neutral-700 dark:bg-neutral-800"
      />
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => onSave(draft)}
          className="rounded-md bg-slate-teal px-2.5 py-1 text-xs text-white hover:opacity-90"
        >
          Save
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-2.5 py-1 text-xs text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
