import { emit, listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { CapacityDots, Chip, Earned, Mono } from "./components/ui";
import { DockActivityView } from "./dock/DockActivityView";
import { DockCapturesView } from "./dock/DockCapturesView";
import { DockNowView } from "./dock/DockNowView";
import { DockTabs, type DockTab } from "./dock/DockTabs";
import { api } from "./lib/api";
import {
  DOCK_SHOWN_EVENT,
  NOW_CHANGED_EVENT,
  SECTIONS_CHANGED_EVENT,
  STARLING_CHANGED_EVENT,
} from "./lib/events";
import type { AgentRow, Capture, ProjectOverview, Section, Session } from "./lib/types";
import { useProjectSignal } from "./state/useProjectSignal";

const DEFAULT_NOW_CAP = 7;

/// "rook-api 3 · 1 waiting" -- queued depth only when there is some, and
/// only the parts that have something to say.
function footerLabel(o: ProjectOverview): string {
  const parts = [o.project_name];
  if (o.now_count > 0) parts[0] += ` ${o.now_count}`;
  if (o.needs_review_count > 0) parts.push(`${o.needs_review_count} needs review`);
  if (o.agents_waiting_count > 0) parts.push(`${o.agents_waiting_count} waiting`);
  return parts.join(" · ");
}

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
  const [agents, setAgents] = useState<AgentRow[]>([]);
  const [tab, setTab] = useState<DockTab>("now");
  const projectSignal = useProjectSignal();
  const [peekedProjectId, setPeekedProjectId] = useState<number | null>(null);
  const effectiveProjectId = peekedProjectId ?? projectSignal.projectId;

  useEffect(() => {
    api
      .getSetting("dock_tab")
      .then((v) => {
        if (v === "now" || v === "captures" || v === "activity") setTab(v);
      })
      .catch(console.error);
  }, []);

  function handleTabChange(next: DockTab) {
    setTab(next);
    api.setSetting("dock_tab", next).catch(console.error);
  }

  const refreshNow = useCallback(() => {
    api.listNow(effectiveProjectId).then(setNow).catch(console.error);
  }, [effectiveProjectId]);

  const refreshSections = useCallback(() => {
    api.listSections().then(setSections).catch(console.error);
  }, []);

  const refreshSessions = useCallback(() => {
    api.listSessions(null).then(setSessions).catch(console.error);
  }, []);

  const refreshOverview = useCallback(() => {
    api.listProjectsOverview().then(setOverview).catch(console.error);
  }, []);

  const refreshAgents = useCallback(() => {
    api
      .listAgents()
      .then((view) => setAgents(view.agents))
      .catch(console.error);
  }, []);

  useEffect(() => {
    refreshNow();
    refreshSections();
    refreshSessions();
    refreshOverview();
    refreshAgents();
    api
      .getSetting("now_cap")
      .then((v) => setNowCap(v ? Number(v) : DEFAULT_NOW_CAP))
      .catch(() => setNowCap(DEFAULT_NOW_CAP));

    // Same tradeoff as the main window's session strip (see App.tsx): an
    // MCP agent is a separate process and can't emit a Tauri event into
    // this window, so sessions/overview -- both of which change from
    // agent-only activity like queue_take -- need a poll, not just event
    // listeners. Starling's agents ride the same poll: asking is cheap (the
    // backend answers from memory and refreshes itself at most every 5s),
    // and a starling:changed event below covers the in-between.
    const poll = setInterval(() => {
      refreshSessions();
      refreshOverview();
      refreshAgents();
    }, 5000);

    const unlisten = listen(NOW_CHANGED_EVENT, () => {
      refreshNow();
      refreshSessions();
      refreshOverview();
    });
    const unlistenStarling = listen(STARLING_CHANGED_EVENT, () => {
      refreshOverview();
      refreshAgents();
    });
    const unlistenSections = listen(SECTIONS_CHANGED_EVENT, refreshSections);
    const unlistenDockShown = listen(DOCK_SHOWN_EVENT, () => setPeekedProjectId(null));
    return () => {
      clearInterval(poll);
      unlisten.then((f) => f());
      unlistenStarling.then((f) => f());
      unlistenSections.then((f) => f());
      unlistenDockShown.then((f) => f());
    };
  }, [refreshNow, refreshSections, refreshSessions, refreshOverview, refreshAgents]);

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
    effectiveProjectId != null
      ? overview.find((o) => o.project_id === effectiveProjectId)
      : undefined;

  // Other real projects with queued work -- the focused project's own
  // depth is already the header chip + main list above, so it's excluded
  // here to avoid showing the same count twice. Earned: absent when
  // there's nothing to report, not shown as an empty footer.
  const otherDepths = overview.filter(
    (o) =>
      o.project_id !== null &&
      o.project_id !== effectiveProjectId &&
      (o.now_count > 0 || o.agents_waiting_count > 0),
  );

  // Starling's agents that magpie has not heard from yet count as live too;
  // the ones it has are already among `sessions`.
  const liveSessionCount =
    sessions.filter((s) => s.ended_at === null).length +
    agents.filter((a) => a.magpie_session === null).length;
  const focusedAgents = agents.filter((a) => a.project_id === effectiveProjectId);

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
          <Chip
            variant="neutral"
            className="min-w-[2.5rem] flex-1"
            title={focused ? `${focused.project_name}${focused.branch ? ` on ${focused.branch}` : ""}` : undefined}
          >
            {focused?.project_name.toUpperCase()}
            {focused?.branch ? ` · ON ${focused.branch.toUpperCase()}` : ""}
          </Chip>
        </Earned>
        <Earned when={liveSessionCount > 0}>
          <Mono size="xs" tone="accent" className="ml-auto shrink-0">
            {liveSessionCount}
            {/* The word goes first when the dock is at its narrowest. */}
            <span className="max-[259px]:hidden"> LIVE</span>
          </Mono>
        </Earned>
      </div>
      <DockTabs active={tab} onChange={handleTabChange} />
      {tab === "now" && (
        <DockNowView
          now={now}
          sections={sections}
          sessions={sessions}
          agents={focusedAgents}
          onAddPrompt={handleAddPrompt}
          onReorder={handleReorder}
          onDone={handleDone}
          onDemote={handleDemote}
          onRevokeLease={handleRevokeLease}
          onRenameSection={handleRenameSection}
          onDeleteSection={handleDeleteSection}
          onReorderSection={handleReorderSection}
        />
      )}
      {tab === "captures" && <DockCapturesView />}
      {tab === "activity" && (
        <DockActivityView projectId={effectiveProjectId} sessions={sessions} />
      )}
      <Earned when={otherDepths.length > 0}>
        <div className="flex flex-wrap items-center gap-1.5 border-t border-hairline px-0.5 pt-1.5">
          {otherDepths.map((o) => (
            <Chip
              key={o.project_id}
              variant={o.needs_review_count > 0 || o.agents_waiting_count > 0 ? "accent" : "neutral"}
              title={footerLabel(o)}
              onClick={() => setPeekedProjectId(o.project_id)}
            >
              {footerLabel(o).toUpperCase()}
            </Chip>
          ))}
        </div>
      </Earned>
    </main>
  );
}

export default DockApp;
