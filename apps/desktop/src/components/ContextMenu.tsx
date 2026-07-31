import { MoreHorizontal } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";

export interface ContextMenuItem {
  label: string;
  onClick?: () => void;
  disabled?: boolean;
  submenu?: ContextMenuItem[];
  destructive?: boolean;
}

interface ContextMenuProps {
  items: ContextMenuItem[];
  /** Imperative open, so a parent's onContextMenu handler and the row's
      keyboard shortcut (Phase 5) can both trigger the same menu instance. */
  openRef?: React.MutableRefObject<((x: number, y: number) => void) | null>;
}

export function ContextMenu({ items, openRef }: ContextMenuProps) {
  const [pos, setPos] = useState<{ x: number; y: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (openRef) openRef.current = (x, y) => setPos({ x, y });
  }, [openRef]);

  useEffect(() => {
    if (!pos) return;
    function onClickAway(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setPos(null);
    }
    document.addEventListener("mousedown", onClickAway);
    return () => document.removeEventListener("mousedown", onClickAway);
  }, [pos]);

  if (!pos) return null;

  return (
    <div
      ref={ref}
      style={{ top: pos.y, left: pos.x }}
      className="fixed z-50 min-w-[180px] rounded-lg border border-neutral-200 bg-white
                 py-1 text-sm shadow-lg dark:border-neutral-700 dark:bg-neutral-900"
    >
      {items.map((item) => (
        <button
          key={item.label}
          type="button"
          disabled={item.disabled}
          onClick={() => {
            item.onClick?.();
            setPos(null);
          }}
          className={cn(
            "flex w-full items-center px-3 py-1.5 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800",
            "disabled:cursor-not-allowed disabled:opacity-40",
            item.destructive && "text-red-600 dark:text-red-400",
          )}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}

/** Hover-reveal trigger button -- matches how the icons it replaces already
    behaved; a permanent per-row control at rest is visual noise a
    minimalist list shouldn't pay for. Right-click itself isn't gated by
    this button at all (the whole row is already the click target). */
export function ContextMenuTrigger({ onOpen }: { onOpen: (x: number, y: number) => void }) {
  return (
    <button
      type="button"
      title="More actions"
      onClick={(e) => {
        const rect = e.currentTarget.getBoundingClientRect();
        onOpen(rect.left, rect.bottom);
      }}
      className="rounded p-1.5 text-neutral-400 opacity-0 hover:bg-neutral-100
                 group-hover:opacity-100 dark:hover:bg-neutral-800"
    >
      <MoreHorizontal size={16} />
    </button>
  );
}
