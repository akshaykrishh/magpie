import { GripVertical, Trash2 } from "lucide-react";
import { useState } from "react";
import type { Section } from "@/lib/types";

interface SectionHeaderProps {
  section: Section;
  onRename: (name: string) => void;
  onDelete: () => void;
  dragHandleProps?: React.HTMLAttributes<HTMLButtonElement>;
}

export function SectionHeader({ section, onRename, onDelete, dragHandleProps }: SectionHeaderProps) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(section.name);

  return (
    <div className="group mt-3 flex items-center gap-1.5 first:mt-0">
      {dragHandleProps && (
        <button
          type="button"
          className="cursor-grab text-neutral-300 hover:text-neutral-500 dark:text-neutral-700"
          {...dragHandleProps}
        >
          <GripVertical size={14} />
        </button>
      )}
      {editing ? (
        <input
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => {
            setEditing(false);
            if (draft.trim() && draft !== section.name) onRename(draft.trim());
          }}
          onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
          className="rounded border border-slate-teal bg-transparent px-1 text-xs font-semibold
                     uppercase tracking-wide text-neutral-500 outline-none dark:text-neutral-400"
        />
      ) : (
        <h3
          onClick={() => setEditing(true)}
          className="cursor-text text-xs font-semibold uppercase tracking-wide text-neutral-500 dark:text-neutral-400"
        >
          {section.name}
        </h3>
      )}
      <button
        type="button"
        onClick={onDelete}
        className="ml-auto rounded p-1 text-neutral-300 opacity-0 hover:bg-red-50 hover:text-red-500
                   group-hover:opacity-100 dark:hover:bg-red-950"
      >
        <Trash2 size={12} />
      </button>
    </div>
  );
}
