import { cn } from "@/lib/utils";

export interface CapacityDotsProps {
  /// How many of `cap` slots are filled (Now's current count).
  filled: number;
  /// Total slots -- the design's Now cap. Always display-only (see
  /// settings.now_cap in the redesign plan's "outruns the data" section);
  /// never used to block promoting past it.
  cap?: number;
  size?: number;
  className?: string;
}

/// The 7-dot capacity meter -- the dock header, the Now column, and
/// Across's per-project rows. Filled dots (leftmost `filled` of them) are
/// solid accent; the rest are a hollow neutral ring, so "how full is this
/// queue" reads at a glance without needing the `n/cap` text next to it
/// (which callers still render separately -- this is the glyph, not a
/// replacement for the label).
export function CapacityDots({ filled, cap = 7, size = 6, className }: CapacityDotsProps) {
  const dots = Array.from({ length: cap }, (_, i) => i < filled);
  return (
    <div className={cn("flex items-center gap-1", className)} role="presentation">
      {dots.map((isFilled, i) => (
        <div
          // eslint-disable-next-line react/no-array-index-key -- dots are
          // positionally meaningful and never reorder/animate individually.
          key={i}
          style={{ width: size, height: size }}
          className={cn(
            "rounded-full",
            isFilled ? "bg-accent" : "border border-hairline-strong",
          )}
        />
      ))}
    </div>
  );
}
