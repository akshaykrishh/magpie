import { emit, listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import { FirstRunView } from "./components/FirstRunView";
import { SessionStrip } from "./components/SessionStrip";
import { TitleBar } from "./components/TitleBar";
import { UndoToast } from "./components/UndoToast";
import { api } from "./lib/api";
import { NOW_CHANGED_EVENT, SECTIONS_CHANGED_EVENT } from "./lib/events";
import { useGlobalKeys } from "./lib/useGlobalKeys";
import type { Capture, Project, Section, SessionView, StreamRow } from "./lib/types";
import { OverlayHost } from "./overlays/OverlayHost";
import { useOverlay } from "./overlays/useOverlay";
import { useProjectSignal } from "./state/useProjectSignal";
import { NowColumn } from "./views/NowColumn";
import { StreamView } from "./views/StreamView";

function App() {
  const overlay = useOverlay();
  const projectSignal = useProjectSignal();
  const [stream, setStream] = useState<StreamRow[]>([]);
  // Distinguishes "genuinely empty" from "failed to load" -- before this,
  // a failed listStreamRows() (e.g. a schema mismatch against the local
  // db) left `stream` at its initial `[]` with the failure visible only in
  // devtools, which read identically to "you have no captures" in the UI.
  const [streamError, setStreamError] = useState<string | null>(null);
  const [now, setNow] = useState<Capture[]>([]);
  const [sessions, setSessions] = useState<SessionView[]>([]);
  const [sections, setSections] = useState<Section[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  // `undefined` = not loaded yet (render nothing rather than flashing the
  // first-run view for a moment on every launch); `null`/`"true"` cover
  // "never set" and "explicitly done" the same way `getSetting` returns
  // them for every other unset-vs-set setting in this app.
  const [onboardingComplete, setOnboardingComplete] = useState<boolean | undefined>(undefined);
  const [undoToast, setUndoToast] = useState<{
    // Identifies *this showing* of a toast, not the deleted item -- two
    // deletes of different templates (or even the same message text twice
    // in a row) must never compare equal here. Used as the React `key` on
    // <UndoToast> below so a second delete forces a genuine remount instead
    // of a prop update: content-based dependency comparison can't tell
    // "same toast, parent re-rendered" apart from "a new toast, same text"
    // when every call site passes the literal string "Template deleted.".
    id: number;
    message: string;
    onUndo: () => void;
  } | null>(null);
  const nextUndoToastId = useRef(0);

  const showUndoToast = useCallback((message: string, onUndo: () => void) => {
    nextUndoToastId.current += 1;
    setUndoToast({ id: nextUndoToastId.current, message, onUndo });
  }, []);

  // Stable identities: `undoToast` itself changes every time a new toast is
  // shown, but these two callbacks must NOT change on every unrelated App
  // re-render (e.g. a background capture arriving while the toast is up) --
  // UndoToast's auto-dismiss timer depends on `onDismiss`'s identity, and a
  // fresh function reference on every render would re-arm the timer forever.
  // The functional `setUndoToast` form reads the current toast at call time,
  // so these can have an empty dependency array.
  const dismissUndoToast = useCallback(() => setUndoToast(null), []);
  const undoAndDismissToast = useCallback(() => {
    setUndoToast((prev) => {
      prev?.onUndo();
      return null;
    });
  }, []);

  const refreshStream = useCallback(() => {
    api
      .listStreamRows({ kind: "all" })
      .then((rows) => {
        setStream(rows);
        setStreamError(null);
      })
      .catch((e) => {
        console.error(e);
        setStreamError(e instanceof Error ? e.message : String(e));
      });
  }, []);

  const refreshNow = useCallback(() => {
    api.listNow(projectSignal.projectId).then(setNow).catch(console.error);
  }, [projectSignal.projectId]);

  const refreshSessions = useCallback(() => {
    api.listSessionsView().then(setSessions).catch(console.error);
  }, []);

  const refreshSections = useCallback(() => {
    api.listSections().then(setSections).catch(console.error);
  }, []);

  const refreshProjects = useCallback(() => {
    api.listProjects().then(setProjects).catch(console.error);
  }, []);

  useEffect(() => {
    refreshStream();
    refreshNow();
    refreshSessions();
    refreshSections();
    refreshProjects();
    api
      .getSetting("onboarding_complete")
      .then((v) => setOnboardingComplete(v === "true"))
      .catch(() => setOnboardingComplete(true));

    // No live push exists for MCP-driven session activity (queue_take,
    // capture_complete, ...) -- an agent's process is separate from this
    // one and can't emit a Tauri event into this window. The rest of the
    // app has the same gap today (see NOW_CHANGED_EVENT's doc comment);
    // for the session strip specifically, a short poll is the same
    // tradeoff the old audit view made (see its own 3s-interval precedent,
    // replaced elsewhere by refresh-on-open) -- a status strip's holds/
    // drained counts are worth keeping roughly live rather than only
    // updating when this window happens to do something else.
    const sessionPoll = setInterval(refreshSessions, 5000);

    const unlistenCapture = listen("capture:added", () => {
      refreshStream();
    });
    // Fires when a capture already in the stream changes in place -- e.g. a
    // tap-to-confirm project assignment from the hotkey flow, or background
    // OCR finishing on a screenshot. Either way the row's data is stale
    // until the stream is refetched.
    const unlistenCaptureUpdated = listen("capture:updated", () => {
      refreshStream();
    });
    // The pinned dock (a separate window) can promote/reorder/complete Now
    // items independently -- this is what keeps this window's copy in sync
    // with changes made over there, and vice versa (see handlers below).
    // Sessions too: a revoke/handback resolved from this window or the
    // dock changes holds/drained immediately, no need to wait on the poll.
    const unlistenNow = listen(NOW_CHANGED_EVENT, () => {
      refreshNow();
      refreshSessions();
    });
    // Same cross-window sync, for sections renamed/reordered/deleted from
    // the dock's own section headers. Must also refresh `stream` here, not
    // just `sections` -- a section delete clears `section_id` on its
    // members server-side, but this window's cached `stream` array still
    // carries the old (now-orphaned) `section_id` until refetched. Without
    // this, `groupBySection` would keep bucketing those captures under a
    // section id that `sections` no longer contains, and they'd render in
    // neither a section group nor the unsectioned list -- silently vanishing
    // from the stream view rather than reappearing as unsectioned.
    const unlistenSections = listen(SECTIONS_CHANGED_EVENT, () => {
      refreshSections();
      refreshStream();
    });
    return () => {
      clearInterval(sessionPoll);
      unlistenCapture.then((f) => f());
      unlistenCaptureUpdated.then((f) => f());
      unlistenNow.then((f) => f());
      unlistenSections.then((f) => f());
    };
  }, [refreshStream, refreshNow, refreshSessions, refreshSections, refreshProjects]);

  useGlobalKeys({
    onOpenPalette: () => overlay.push({ kind: "palette" }),
    onOpenActivity: () => overlay.push({ kind: "activity" }),
  });

  async function handlePromote(id: number) {
    await api.promoteCapture(id);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleAddPrompt(body: string) {
    await api.addTypedCapture(body);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleNowReorder(id: number, afterId: number | null) {
    // Optimistic: reflect the new order immediately, then reconcile with
    // the server's actual fractional-index result.
    setNow((prev) => {
      const items = [...prev];
      const from = items.findIndex((c) => c.id === id);
      if (from === -1) return prev;
      const [moved] = items.splice(from, 1);
      const afterIndex = afterId === null ? -1 : items.findIndex((c) => c.id === afterId);
      items.splice(afterIndex + 1, 0, moved);
      return items;
    });
    await api.reorderCapture(id, afterId);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleRenameSection(id: number, name: string) {
    await api.renameSection(id, name);
    refreshSections();
    emit(SECTIONS_CHANGED_EVENT);
  }

  async function handleDeleteSection(id: number) {
    await api.deleteSection(id);
    // Deleting a section only clears its members' section_id -- the
    // captures themselves aren't touched, so both lists need a refetch to
    // pick up their new (unsectioned) membership.
    refreshSections();
    refreshStream();
    refreshNow();
    emit(SECTIONS_CHANGED_EVENT);
    emit(NOW_CHANGED_EVENT);
  }

  async function handleReorderSection(id: number, afterId: number | null) {
    setSections((prev) => {
      const items = [...prev];
      const from = items.findIndex((s) => s.id === id);
      if (from === -1) return prev;
      const [moved] = items.splice(from, 1);
      const afterIndex = afterId === null ? -1 : items.findIndex((s) => s.id === afterId);
      items.splice(afterIndex + 1, 0, moved);
      return items;
    });
    await api.reorderSection(id, afterId);
    refreshSections();
    emit(SECTIONS_CHANGED_EVENT);
  }

  async function handleDone(id: number) {
    await api.markCaptureDone(id);
    // Now-list callers already got away with skipping this (a Now row
    // disappears from `now` either way), but Mark Done/Reopen is now also
    // reachable from the main stream's context menu, where the row's
    // Reopen/Mark Done toggle is read straight off `stream`'s own copy of
    // done_at -- that needs a refetch too, or the toggle would stay stuck on
    // whichever label it showed before the click.
    refreshStream();
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleReopen(id: number) {
    await api.reopenCapture(id);
    refreshStream();
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleDemote(id: number) {
    await api.demoteCapture(id);
    refreshNow();
    refreshStream();
    emit(NOW_CHANGED_EVENT);
  }

  function handleTemplateInstantiated() {
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  function handleCaptureResolved() {
    refreshStream();
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  // Undefined while the setting is still loading -- renders nothing
  // rather than flashing FirstRunView for a moment on every ordinary
  // launch (see `onboardingComplete`'s own doc comment).
  if (onboardingComplete === undefined) return null;
  if (!onboardingComplete) {
    return <FirstRunView onDone={() => setOnboardingComplete(true)} />;
  }

  return (
    <main className="relative flex h-screen flex-col overflow-hidden">
      <TitleBar onOpenPalette={() => overlay.push({ kind: "palette" })} />
      <SessionStrip sessions={sessions} />
      <div className="flex flex-1 overflow-hidden">
        <NowColumn
          now={now}
          sections={sections}
          onAddPrompt={handleAddPrompt}
          onReorder={handleNowReorder}
          onDone={handleDone}
          onDemote={handleDemote}
          onRenameSection={handleRenameSection}
          onDeleteSection={handleDeleteSection}
          onReorderSection={handleReorderSection}
          onTemplateInstantiated={handleTemplateInstantiated}
          onLeaseRevoked={refreshNow}
        />

        <section className="flex flex-1 flex-col overflow-hidden">
          <StreamView
            stream={stream}
            streamError={streamError}
            sections={sections}
            projects={projects}
            refreshStream={refreshStream}
            refreshNow={refreshNow}
            refreshSections={refreshSections}
            showUndoToast={showUndoToast}
            onPromote={handlePromote}
            onDone={handleDone}
            onReopen={handleReopen}
            onRenameSection={handleRenameSection}
            onDeleteSection={handleDeleteSection}
            onReorderSection={handleReorderSection}
          />
        </section>
      </div>

      {undoToast && (
        <UndoToast
          // `key` forces a fresh mount (and thus a fresh 6s timer) whenever a
          // logically new toast replaces the one being shown, even if the
          // message text is identical -- see the comment on `undoToast`'s
          // `id` field above.
          key={undoToast.id}
          message={undoToast.message}
          onUndo={undoAndDismissToast}
          onDismiss={dismissUndoToast}
        />
      )}

      <OverlayHost
        onInstantiated={handleTemplateInstantiated}
        onShowUndo={showUndoToast}
        onCaptureResolved={handleCaptureResolved}
        projectCount={projects.length}
      />
    </main>
  );
}

export default App;
