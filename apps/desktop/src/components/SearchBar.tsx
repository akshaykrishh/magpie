import { Search, X } from "lucide-react";

interface SearchBarProps {
  value: string;
  onChange: (value: string) => void;
}

export function SearchBar({ value, onChange }: SearchBarProps) {
  return (
    <div className="relative">
      <Search
        size={16}
        className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-neutral-400"
      />
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Search captures…"
        className="w-full rounded-lg border border-neutral-200 bg-white py-2 pl-9 pr-8 text-sm
                   text-neutral-800 placeholder:text-neutral-400 focus:border-blue-400
                   focus:outline-none dark:border-neutral-800 dark:bg-neutral-900
                   dark:text-neutral-200"
      />
      {value && (
        <button
          type="button"
          onClick={() => onChange("")}
          className="absolute right-2.5 top-1/2 -translate-y-1/2 text-neutral-400 hover:text-neutral-600"
        >
          <X size={16} />
        </button>
      )}
    </div>
  );
}
