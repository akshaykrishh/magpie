import { cn } from "@/lib/utils";

export interface ProgressBarProps {
  /// 0-1. Clamped defensively -- callers computing this from `filled/cap`
  /// with a degenerate cap (e.g. now_cap set to 0) must not produce a bar
  /// wider than its track.
  value: number;
  className?: string;
}

/// The 2px accent progress bar under the Now column's header ("5/7").
/// Track is a hairline; fill is solid accent. No animation on value change
/// -- this reflects state, it doesn't narrate the transition.
export function ProgressBar({ value, className }: ProgressBarProps) {
  const pct = Math.max(0, Math.min(1, value)) * 100;
  return (
    <div
      role="presentation"
      className={cn("h-0.5 overflow-hidden rounded-full bg-hairline", className)}
    >
      <div className="h-full bg-accent" style={{ width: `${pct}%` }} />
    </div>
  );
}
