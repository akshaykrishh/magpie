import { cn } from "@/lib/utils";

export type DiamondMarkState = "solid" | "hollow" | "muted";

export interface DiamondMarkProps {
  state?: DiamondMarkState;
  size?: number;
  /// Plays the one 220ms scale-in on mount -- use only for a genuine
  /// first appearance (a fresh toast, a session card arriving), never on
  /// every re-render of an already-visible mark. See index.css's
  /// `mp-scale-in` keyframe doc comment.
  animateIn?: boolean;
  className?: string;
}

/// The rotated-45deg square that is the app's mark everywhere: the toast,
/// the tray, the dock header, the title bar's project chip. Three states
/// mirror the toast's certain/error visual language so a "solid accent"
/// mark always means the same thing wherever it appears:
///   - solid:  filled with --color-accent (certain / affirmative)
///   - hollow: accent-colored outline only, transparent fill (chord /
///             "into Now" -- an action taken, not a fact stated)
///   - muted:  faint neutral fill, no accent (error / nothing state)
export function DiamondMark({
  state = "solid",
  size = 16,
  animateIn = false,
  className,
}: DiamondMarkProps) {
  return (
    <div
      role="presentation"
      style={{ width: size, height: size }}
      className={cn(
        "shrink-0 rotate-45 rounded-[2px]",
        state === "solid" && "bg-accent",
        state === "hollow" && "border-[1.5px] border-accent bg-transparent",
        state === "muted" && "bg-fg-faint",
        animateIn && "animate-mp-scale-in",
        className,
      )}
    />
  );
}
