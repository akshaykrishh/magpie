import { Plus } from "lucide-react";
import { useState } from "react";

interface AddPromptInputProps {
  onAdd: (body: string) => void;
}

export function AddPromptInput({ onAdd }: AddPromptInputProps) {
  const [value, setValue] = useState("");

  function submit() {
    const trimmed = value.trim();
    if (!trimmed) return;
    onAdd(trimmed);
    setValue("");
  }

  return (
    <div className="flex items-center gap-2">
      <input
        type="text"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") submit();
        }}
        placeholder="Type a prompt to queue…"
        className="flex-1 rounded-lg border border-neutral-200 bg-white px-3 py-2 text-sm
                   text-neutral-800 placeholder:text-neutral-400 focus:border-slate-teal
                   focus:outline-none dark:border-neutral-800 dark:bg-neutral-900
                   dark:text-neutral-200"
      />
      <button
        type="button"
        onClick={submit}
        disabled={!value.trim()}
        className="flex items-center justify-center rounded-lg bg-slate-teal p-2 text-white
                   hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40"
      >
        <Plus size={16} />
      </button>
    </div>
  );
}
