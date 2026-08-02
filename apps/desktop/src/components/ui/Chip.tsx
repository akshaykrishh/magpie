import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Mono } from "./Mono";

export type ChipVariant = "neutral" | "accent" | "solid";

export interface ChipProps {
  children: ReactNode;
  variant?: ChipVariant;
  size?: "sm" | "md";
  className?: string;
  onClick?: () => void;
}

const VARIANT_CLASS: Record<ChipVariant, string> = {
  // `MAGPIE-CORE — CERTAIN`, `MERGED ×3` -- a stated fact, no call to action.
  neutral: "border border-hairline-strong",
  // `⌥⏎ REFILE` -- an available action, not yet taken.
  accent: "border border-accent-line",
  // `⏎ REVIEW` -- the primary action for a row that needs one.
  solid: "bg-accent border border-accent",
};

/// A small bordered/solid mono label -- provenance chips (`MAGPIE-CORE —
/// CERTAIN`), lease badges (`LEASED BY S1 · 12M`), and action affordances
/// (`⏎ REVIEW`, `⌥⏎ REFILE`). One component so the three visual weights
/// (stated fact / available action / primary action) stay consistent
/// everywhere a chip appears, rather than each row type inventing its own
/// border+padding combination.
export function Chip({ children, variant = "neutral", size = "sm", className, onClick }: ChipProps) {
  const Tag = onClick ? "button" : "span";
  return (
    <Tag
      type={onClick ? "button" : undefined}
      onClick={onClick}
      className={cn(
        "inline-flex items-center gap-1 rounded-xs",
        size === "sm" ? "px-1.5 py-0.5" : "px-2 py-1",
        onClick && "cursor-pointer hover:opacity-80",
        VARIANT_CLASS[variant],
        className,
      )}
    >
      <Mono size={size === "sm" ? "sm" : "md"} tone={variant === "solid" ? "on-accent" : variant === "accent" ? "accent" : "muted"}>
        {children}
      </Mono>
    </Tag>
  );
}
