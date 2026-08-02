import { useEffect } from "react";
import { SheetOverlay } from "@/components/ui";
import { ActivitySheet } from "./ActivitySheet";
import { CommandPalette } from "./CommandPalette";
import { ReviewSheet } from "./ReviewSheet";
import { TemplatesSheet } from "./TemplatesSheet";
import { TrashSheet } from "./TrashSheet";
import { useOverlay } from "./useOverlay";

interface OverlayHostProps {
  onInstantiated: () => void;
  onShowUndo: (message: string, onUndo: () => void) => void;
  /** Refetch stream/Now after the review sheet's Accept/Send back --
      same "mutate then refetch" contract as every other handler. */
  onCaptureResolved: () => void;
  /** How many real projects exist -- threaded down to CommandPalette's
      Across entry, which needs it to decide whether it's runnable. */
  projectCount: number;
}

/// Renders only the TOP of the overlay stack -- a true stack, not a pile
/// of simultaneously-mounted layers, so there's exactly one scrim, one
/// focus trap (via SheetOverlay), and one Escape handler regardless of
/// how deep the stack gets. Popping remounts whatever's now on top, which
/// is fine: every entry here fetches its own data on mount rather than
/// needing to preserve scroll position or draft state across a pop.
export function OverlayHost({
  onInstantiated,
  onShowUndo,
  onCaptureResolved,
  projectCount,
}: OverlayHostProps) {
  const { stack, pop } = useOverlay();
  const top = stack[stack.length - 1];

  useEffect(() => {
    if (!top) return;
    function handler(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        pop();
      }
    }
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [top, pop]);

  if (!top) return null;

  if (top.kind === "activity") {
    return (
      <SheetOverlay onDismiss={pop} align="right" panelClassName="w-[420px] h-full">
        <ActivitySheet />
      </SheetOverlay>
    );
  }

  return (
    <SheetOverlay onDismiss={pop} align="center" panelClassName="w-[560px]">
      {top.kind === "palette" && <CommandPalette projectCount={projectCount} />}
      {top.kind === "trash" && <TrashSheet />}
      {top.kind === "templates" && (
        <TemplatesSheet onInstantiated={onInstantiated} onShowUndo={onShowUndo} />
      )}
      {top.kind === "review" && (
        <ReviewSheet capture={top.capture} onResolved={onCaptureResolved} />
      )}
    </SheetOverlay>
  );
}
