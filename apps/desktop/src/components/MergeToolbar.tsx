import { Combine, X } from "lucide-react";

interface MergeToolbarProps {
  count: number;
  onMerge: () => void;
  onClear: () => void;
}

export function MergeToolbar({ count, onMerge, onClear }: MergeToolbarProps) {
  if (count === 0) return null;

  return (
    <div className="flex items-center justify-between rounded-lg bg-slate-teal/10 px-3 py-2 text-sm dark:bg-slate-teal-light/15">
      <span className="text-slate-teal dark:text-slate-teal-light">{count} selected</span>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onMerge}
          disabled={count < 2}
          className="flex items-center gap-1.5 rounded-md bg-slate-teal px-2.5 py-1 text-white
                     hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Combine size={14} />
          Merge
        </button>
        <button
          type="button"
          onClick={onClear}
          className="rounded-md p-1 text-slate-teal hover:bg-slate-teal/15 dark:text-slate-teal-light dark:hover:bg-slate-teal-light/20"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
