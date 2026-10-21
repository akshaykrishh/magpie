import { useEffect } from "react";
import { MoreHorizontal } from "lucide-react";
import { popupMenu, type ContextMenuItem } from "@/lib/nativeMenu";

export type { ContextMenuItem } from "@/lib/nativeMenu";

interface ContextMenuProps {
  items: ContextMenuItem[];
  /** Imperative open, so a parent's onContextMenu handler, the "..." trigger
      and the row's keyboard shortcut can all trigger the same menu. */
  openRef?: React.MutableRefObject<((x: number, y: number) => void) | null>;
}

/// Draws nothing: the menu is a native OS menu (see lib/nativeMenu.ts), so
/// this only wires `openRef` to pop it up with the *current* items. Kept as
/// a component so each call site's items and ref stay declared where they
/// were.
export function ContextMenu({ items, openRef }: ContextMenuProps) {
  // Wired after render, not during it, so render stays pure. Runs on every
  // commit (no deps) so the opener always sees the latest `items`; a menu
  // can only be opened from an event, which fires after the effect.
  useEffect(() => {
    if (!openRef) return;
    openRef.current = (x, y) => {
      popupMenu(items, { x, y }).catch(console.error);
    };
  });
  return null;
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
