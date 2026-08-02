import { useEffect } from "react";

export interface GlobalKeyActions {
  onOpenPalette: () => void;
  onOpenActivity: () => void;
}

/// Global keyboard shortcuts that fire regardless of what has focus --
/// unlike useListCursor's single-key dispatch, these are modifier chords
/// (⌘K, ⌘L), so there's no risk of intercepting a letter someone is
/// actually typing (a bare "k" keypress carries no ⌘, so it never reaches
/// this handler's branches at all) -- no `isTextEntryTarget` guard needed,
/// matching how command palettes conventionally behave elsewhere (Slack,
/// Notion, Linear: ⌘K works even from inside a text field).
///
/// Currently owns just ⌘K (command palette) and ⌘L (activity) -- the
/// design's other global shortcuts (⌘M merge, ⌥⏎ refile, ⌥P pin, ⌥U
/// triage, ⌘⇧D toggle dock, ⌥⇥ switch project) are wired in as the actions
/// they trigger land in later stages, not stubbed here ahead of them (see
/// the redesign plan's "don't ship dead code before the gesture it drives
/// exists").
export function useGlobalKeys({ onOpenPalette, onOpenActivity }: GlobalKeyActions) {
  useEffect(() => {
    function handler(e: KeyboardEvent) {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      const key = e.key.toLowerCase();
      if (key === "k") {
        e.preventDefault();
        onOpenPalette();
      } else if (key === "l") {
        e.preventDefault();
        onOpenActivity();
      }
    }
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onOpenPalette, onOpenActivity]);
}
