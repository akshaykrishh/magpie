import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

export interface FloatingSurfaceProps {
  children: ReactNode;
  /// "float" is the toast/aim-panel weight (small, pill-ish, drop shadow
  /// only) -- see design tokens `--mp-shadow-float`. "sheet" is the
  /// heavier weight for a centered palette or Across (`--mp-shadow-sheet`).
  weight?: "float" | "sheet";
  className?: string;
}

/// The floating-panel shell shared by every surface that isn't docked into
/// a window's normal layout: the aim panel, Across, the command palette,
/// and (conceptually) the toast, though the toast window ships without
/// React or Tailwind (see src/toast.ts) and reimplements this same look in
/// plain CSS -- src/styles/toast.css -- rather than importing this file.
///
/// Deliberately just a styled div, no positioning/portal/focus-trap logic
/// -- those differ per surface (a Tauri window needs none of it; an
/// in-window overlay needs a scrim + focus trap, owned by
/// overlays/OverlayHost.tsx, not here). This component is only the visual
/// vocabulary: translucent overlay background, hairline border, blur,
/// shadow, radius.
export function FloatingSurface({ children, weight = "float", className }: FloatingSurfaceProps) {
  return (
    <div
      className={cn(
        "rounded-lg border border-hairline-strong bg-overlay backdrop-blur-xl",
        weight === "float" ? "shadow-float" : "shadow-sheet",
        className,
      )}
    >
      {children}
    </div>
  );
}
