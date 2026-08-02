import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

const SIZE_CLASS = {
  xs: "text-label-xs",
  sm: "text-label-sm",
  md: "text-label",
  lg: "text-label-lg",
} as const;

const TONE_CLASS = {
  fg: "text-fg",
  muted: "text-fg-muted",
  faint: "text-fg-faint",
  accent: "text-accent",
  "on-accent": "text-fg-on-accent",
  danger: "text-danger",
} as const;

export interface MonoProps {
  children: ReactNode;
  size?: keyof typeof SIZE_CLASS;
  tone?: keyof typeof TONE_CLASS;
  /** Uppercase + wide letter-spacing -- the design's metadata-label look
      (session badges, provenance chips, status-bar hints: `LEASED BY S1 ·
      12M`, `MAGPIE-CORE — CERTAIN`). Off for mono text that isn't a label,
      e.g. a keybinding rendered inline in a sentence. */
  wide?: boolean;
  weight?: "normal" | "medium" | "bold";
  className?: string;
}

const WEIGHT_CLASS = {
  normal: "font-normal",
  medium: "font-medium",
  bold: "font-bold",
} as const;

/// The 9-11.5px uppercase, letter-spaced JetBrains Mono label used for
/// every piece of metadata in the app -- session badges, provenance chips,
/// timestamps, status-bar hints. Every mono label in the redesign should
/// render through this rather than a one-off `font-mono-label text-[10px]
/// uppercase tracking-...` class string, so the four label sizes in
/// index.css's `@theme` stay the only four sizes that ever appear.
export function Mono({
  children,
  size = "md",
  tone = "muted",
  wide = true,
  weight = "normal",
  className,
}: MonoProps) {
  return (
    <span
      className={cn(
        "font-mono-label",
        SIZE_CLASS[size],
        TONE_CLASS[tone],
        WEIGHT_CLASS[weight],
        wide && "uppercase tracking-label",
        className,
      )}
    >
      {children}
    </span>
  );
}
