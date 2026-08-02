import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface KbdProps {
  children: ReactNode;
  className?: string;
}

/// A keybinding chip -- `⌘K`, `⏎`, `⌥⌫`, `⇧⏎ NOW`. Used in the status bar,
/// row hover affordances, and the tray/palette. Deliberately has no
/// interactive affordance of its own (no hover state, no button semantics)
/// -- it's a label for a shortcut that fires globally or on its row, not a
/// clickable control; wrap it in a real <button> at the call site if a
/// given instance is meant to be clickable (e.g. the review sheet's
/// `⏎ REVIEW` chip, which both labels and triggers the action).
export function Kbd({ children, className }: KbdProps) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-0.5 rounded-xs border border-hairline-strong px-1.5 py-0.5",
        "font-mono-label text-label-sm text-fg-muted",
        className,
      )}
    >
      {children}
    </span>
  );
}
