import { emit, listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { AddPromptInput } from "./components/AddPromptInput";
import { NowList } from "./components/NowList";
import { CapacityDots, Chip, Earned, Mono } from "./components/ui";
import { api } from "./lib/api";
import { NOW_CHANGED_EVENT, SECTIONS_CHANGED_EVENT } from "./lib/events";
import type { Capture, ProjectOverview, Section, Session } from "./lib/types";
import { useProjectSignal } from "./state/useProjectSignal";

const DEFAULT_NOW_CAP = 7;

/// The pinned dock: a compact, always-on-top view of Now, meant to sit
/// beside whatever you're actually working in. Deliberately just Now --
/// the stream/search/merge surface lives in the main window, which this
/// is not a replacement for.
function DockApp() {
  const [now, setNow] = useState<Capture[]>([]);
  const [sections, setSections] = useState<Section[]>([]);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [nowCap, setNowCap] = useState(DEFAULT_NOW_CAP);
  const [overview, setOverview] = useState<ProjectOverview[]>([]);
  const projectSignal = useProjectSignal();

  const refreshNow = useCallback(() => {
    api.listNow(null).then(setNow).catch(console.error);
  }, []);

  const refreshSections = useCallback(() => {
    api.listSections().then(setSections).catch(console.error);
  }, []);

  const refreshSessions = useCallback(() => {
    api.listSessions(null).then(setSessions).catch(console.error);
  }, []);

  const refreshOverview = useCallback(() => {
    api.listProjectsOverview().then(setOverview).catch(console.error);
  }, []);

  useEffect(() => {
    refreshNow();
    refreshSections();
    refreshSessions();
    refreshOverview();
    api
      .getSetting("now_cap")
      .then((v) => setNowCap(v ? Number(v) : DEFAULT_NOW_CAP))
      .catch(() => setNowCap(DEFAULT_NOW_CAP));

    // Same tradeoff as the main window's session strip (see App.tsx): an
    // MCP agent is a separate process and can't emit a Tauri event into
    // this window, so sessions/overview -- both of which change from
    // agent-only activity like queue_take -- need a poll, not just event
    // listeners.
    const poll = setInterval(() => {
      refreshSessions();
      refreshOverview();
    }, 5000);

    const unlisten = listen(NOW_CHANGED_EVENT, () => {
      refreshNow();
      refreshSessions();
      refreshOverview();
    });
    const unlistenSections = listen(SECTIONS_CHANGED_EVENT, refreshSections);
    return () => {
      clearInterval(poll);
      unlisten.then((f) => f());
      unlistenSections.then((f) => f());
    };
  }, [refreshNow, refreshSections, refreshSessions, refreshOverview]);

  async function handleAddPrompt(body: string) {
    await api.addTypedCapture(body);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleReorder(id: number, afterId: number | null) {
    await api.reorderCapture(id, afterId);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleDone(id: number) {
    await api.markCaptureDone(id);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleDemote(id: number) {
    await api.demoteCapture(id);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleRevokeLease(id: number) {
    await api.revokeLease(id);
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
    refreshSections();
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

  // Same derivation useProjectSignal already does for the main window's
  // project chip -- an explicit pin wins, otherwise "exactly one project
  // has a live session". Looked up against `overview` (not `listProjects`)
  // since that's what already carries `branch` alongside the name.
  const focused =
    projectSignal.projectId != null
      ? overview.find((o) => o.project_id === projectSignal.projectId)
      : undefined;

  // Other real projects with queued work -- context the dock's flat,
  // unscoped Now list can't otherwise show. Earned: absent when there's
  // nothing to report, not shown as an empty footer.
  const otherDepths = overview.filter((o) => o.project_id !== null && o.now_count > 0);

  const liveSessionCount = sessions.filter((s) => s.ended_at === null).length;

  return (
    <main className="flex h-screen flex-col gap-2 overflow-hidden bg-white/95 p-2 dark:bg-neutral-950/95">
      <div className="flex items-center gap-1.5 px-0.5">
        <div className="flex shrink-0 items-center gap-1.5">
          <CapacityDots filled={now.length} cap={nowCap} />
          <Mono size="xs" tone="faint">
            {now.length}/{nowCap}
          </Mono>
        </div>
        {/* min-w-0 is what lets a flex item shrink below its content's
            width at all; truncate then ellipsizes instead of wrapping,
            which is what happens to a long project + branch name in a
            300px dock without both. */}
        <Earned when={focused != null}>
          <Chip variant="neutral" className="min-w-0 flex-1 truncate">
            {focused?.project_name.toUpperCase()}
            {focused?.branch ? ` · ON ${focused.branch.toUpperCase()}` : ""}
          </Chip>
        </Earned>
        <Earned when={liveSessionCount > 0}>
          <Mono size="xs" tone="accent" className="ml-auto shrink-0">
            {liveSessionCount} LIVE
          </Mono>
        </Earned>
      </div>
      <AddPromptInput onAdd={handleAddPrompt} />
      <div className="flex-1 overflow-y-auto">
        <NowList
          items={now}
          onReorder={handleReorder}
          onDone={handleDone}
          onDemote={handleDemote}
          sections={sections}
          onRenameSection={handleRenameSection}
          onDeleteSection={handleDeleteSection}
          onReorderSection={handleReorderSection}
          sessions={sessions}
          onRevokeLease={handleRevokeLease}
        />
      </div>
      <Earned when={otherDepths.length > 0}>
        <div className="flex flex-wrap items-center gap-1.5 border-t border-hairline px-0.5 pt-1.5">
          {otherDepths.map((o) => (
            <Mono key={o.project_id} size="xs" tone="faint">
              {o.project_name.toUpperCase()} {o.now_count}
            </Mono>
          ))}
        </div>
      </Earned>
    </main>
  );
}

export default DockApp;
