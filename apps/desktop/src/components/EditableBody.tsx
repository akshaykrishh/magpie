import { useState } from "react";
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
