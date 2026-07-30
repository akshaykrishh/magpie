import { Combine, X } from "lucide-react";

interface MergeToolbarProps {
  count: number;
  onMerge: () => void;
  onClear: () => void;
}

export function MergeToolbar({ count, onMerge, onClear }: MergeToolbarProps) {
  if (count === 0) return null;

  return (
    <div className="flex items-center justify-between rounded-lg bg-blue-50 px-3 py-2 text-sm dark:bg-blue-950">
      <span className="text-blue-700 dark:text-blue-300">{count} selected</span>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onMerge}
          disabled={count < 2}
          className="flex items-center gap-1.5 rounded-md bg-blue-600 px-2.5 py-1 text-white
                     hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Combine size={14} />
          Merge
        </button>
        <button
          type="button"
          onClick={onClear}
          className="rounded-md p-1 text-blue-700 hover:bg-blue-100 dark:text-blue-300 dark:hover:bg-blue-900"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
