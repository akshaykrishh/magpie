import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { FloatingSurface } from "./FloatingSurface";

export interface SheetOverlayProps {
  children: ReactNode;
  /// Scrim click dismisses -- the actual Escape/stack-pop handling lives
  /// in overlays/OverlayHost.tsx (one handler for the whole stack, per the
  /// redesign plan), not here. This is deliberately presentation-only so
  /// OverlayHost stays the single place that owns focus-trap and
  /// keyboard-dismiss semantics across every sheet kind.
  onDismiss: () => void;
  /// "center" is the palette/authoring-sheet placement (Templates
  /// authoring, Trash). "right" is Activity's "slides over the stream"
  /// placement -- callers position/size the panel itself via
  /// `panelClassName` (e.g. constraining it to the stream pane's width),
  /// since that's app-layout knowledge a bare primitive shouldn't encode.
  align?: "center" | "right";
  panelClassName?: string;
  className?: string;
}

/// The scrim + floating panel shared by every in-window overlay (command
/// palette, Activity, Trash, triage, the hand-back review sheet, the
/// expanded-capture view). One visual shell so all of them dim the
/// background and float the same way; OverlayHost stacks these rather than
/// each overlay managing its own scrim.
export function SheetOverlay({
  children,
  onDismiss,
  align = "center",
  panelClassName,
  className,
}: SheetOverlayProps) {
  return (
    <div
      className={cn("absolute inset-0 z-40 flex bg-black/40", className)}
      onClick={(e) => {
        if (e.target === e.currentTarget) onDismiss();
      }}
    >
      <FloatingSurface
        weight="sheet"
        className={cn(
          "m-auto max-h-[85%] overflow-auto",
          align === "right" && "mr-0 ml-auto h-full rounded-r-none",
          panelClassName,
        )}
      >
        {children}
      </FloatingSurface>
    </div>
  );
}
