import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { Capabilities } from "@/lib/types";

/// Today's heuristic this replaces counted stream length (>=3) and fired
/// after the third capture of *any* kind. This counts only clipboard-only
/// captures specifically (see capture_flow.rs's `clipboard_only_capture_count`
/// bump) and waits for the friction to actually be felt repeatedly, not
/// just for the stream to have a few rows in it from other sources
/// (screenshots, typed prompts, agent captures never go through this mode
/// at all).
const CLIPBOARD_ONLY_CAPTURES_BEFORE_OFFER = 24;

/// Backs both the permission card (stream's first slot, StreamView.tsx)
/// and its ⌘K fallback (CommandPalette.tsx) from one source of truth.
///
/// `dismissed` only retires the *card* -- `eligible` (once true, stays
/// true: the count only grows) is what CommandPalette gates on, so the
/// upgrade is never permanently lost, only demoted from unsolicited to
/// on-demand. Matches the redesign plan's "dismiss is permanent [for the
/// card] and moves it into ⌘K."
export function usePermissionOffer() {
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const [clipboardOnlyCount, setClipboardOnlyCount] = useState(0);
  const [dismissed, setDismissed] = useState(false);

  const refresh = useCallback(() => {
    api.captureCapabilities().then(setCapabilities).catch(console.error);
    api
      .getSetting("clipboard_only_capture_count")
      .then((v) => setClipboardOnlyCount(v ? Number(v) : 0))
      .catch(() => setClipboardOnlyCount(0));
    api
      .getSetting("permission_offer_dismissed")
      .then((v) => setDismissed(v === "true"))
      .catch(() => setDismissed(false));
  }, []);

  useEffect(() => {
    refresh();
    // Capability state can change mid-session (granting Accessibility
    // doesn't require a relaunch -- see App.tsx's identical comment on its
    // own capture_capabilities refetch), and the clipboard-only counter
    // only changes via a capture -- both ride "capture:added" (used
    // inline, not via a named constant -- see lib/events.ts's own comment
    // on why that one stays inline).
    const unlisten = listen("capture:added", refresh);
    return () => {
      unlisten.then((f) => f());
    };
  }, [refresh]);

  const canUpgrade =
    capabilities?.mode === "clipboard_only" && capabilities.synthesized_copy_available;
  const eligible = canUpgrade && clipboardOnlyCount >= CLIPBOARD_ONLY_CAPTURES_BEFORE_OFFER;

  const dismiss = useCallback(() => {
    setDismissed(true);
    api.setSetting("permission_offer_dismissed", "true").catch(console.error);
  }, []);

  const upgrade = useCallback(() => {
    api.openAccessibilitySettings().catch(console.error);
  }, []);

  return {
    /// The loud, first-stream-slot card -- earned at 24 clipboard-only
    /// captures, gone for good (in this form) once dismissed.
    showCard: eligible && !dismissed,
    /// The quiet ⌘K fallback -- available for as long as `eligible` is
    /// true, dismissed or not.
    showInPalette: eligible,
    dismiss,
    upgrade,
  };
}
