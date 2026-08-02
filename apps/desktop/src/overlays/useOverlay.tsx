import { createContext, useCallback, useContext, useState, type ReactNode } from "react";
import type { Capture } from "@/lib/types";

// The design's remaining overlay-ish surfaces (triage, resume, the
// expanded-capture view) join this union in the stages that build their
// content -- see the redesign plan: adding a case here ahead of a real
// consumer is the "don't ship dead code" mistake this project keeps
// avoiding elsewhere. ExpandedCaptureModal in particular stays
// CaptureRow-local rather than folding in here (see that component's doc
// comment for why).
export type OverlayEntry =
  | { kind: "palette" }
  | { kind: "activity" }
  | { kind: "trash" }
  | { kind: "templates" }
  // Carries the capture itself, not just an id -- the caller (CaptureRow's
  // ⏎ REVIEW chip) already has the full row in hand, and there's no
  // `get_capture`-by-id command exposed to IPC to refetch it from. If the
  // sheet is left open while the underlying row changes elsewhere, its
  // copy goes stale until reopened -- acceptable for a sheet that's
  // opened, acted on, and closed in one sitting, same tradeoff
  // ExpandedCaptureModal already makes.
  | { kind: "review"; capture: Capture };

interface OverlayContextValue {
  stack: OverlayEntry[];
  push: (entry: OverlayEntry) => void;
  /** Replaces the top of the stack rather than adding to it -- what a
      command palette selection does everywhere it's a convention (VS
      Code, Linear): picking "Manage templates" opens the templates sheet
      in the palette's place, it doesn't leave the palette as a "back"
      destination underneath. */
  replace: (entry: OverlayEntry) => void;
  pop: () => void;
  popAll: () => void;
}

const OverlayContext = createContext<OverlayContextValue | null>(null);

export function OverlayProvider({ children }: { children: ReactNode }) {
  const [stack, setStack] = useState<OverlayEntry[]>([]);
  const push = useCallback((entry: OverlayEntry) => setStack((s) => [...s, entry]), []);
  const replace = useCallback(
    (entry: OverlayEntry) => setStack((s) => [...s.slice(0, -1), entry]),
    [],
  );
  const pop = useCallback(() => setStack((s) => s.slice(0, -1)), []);
  const popAll = useCallback(() => setStack([]), []);
  return (
    <OverlayContext.Provider value={{ stack, push, replace, pop, popAll }}>
      {children}
    </OverlayContext.Provider>
  );
}

export function useOverlay() {
  const ctx = useContext(OverlayContext);
  if (!ctx) throw new Error("useOverlay must be used within an OverlayProvider");
  return ctx;
}
