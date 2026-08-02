import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { emit } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CaptureRow } from "@/components/CaptureRow";
import { MergeToolbar } from "@/components/MergeToolbar";
import { PermissionCard } from "@/components/PermissionCard";
import { SectionHeader } from "@/components/SectionHeader";
import { Chip, Earned, Mono } from "@/components/ui";
import { api } from "@/lib/api";
import { NOW_CHANGED_EVENT, SECTIONS_CHANGED_EVENT } from "@/lib/events";
import { groupBySection } from "@/lib/grouping";
import { isTextEntryTarget } from "@/lib/keys";
import type { Capture, Project, Section, StreamRow } from "@/lib/types";
import { useListCursor } from "@/lib/useListCursor";
import { usePermissionOffer } from "@/state/usePermissionOffer";
import { useProjectSignal } from "@/state/useProjectSignal";

/// A stream row is either a search result (plain `Capture` -- `search_captures`
/// isn't joined, see api.ts) or a real stream row (`StreamRow`, joined --
/// see stream.rs). One render path handles both via this narrowing, rather
/// than fabricating a fake `StreamRow` for search results (which would
/// have to lie about `blob_mime`, silently breaking screenshot rendering
/// for search hits -- see CaptureRow's own fallback blob-fetch path,
/// which only activates correctly when `streamRow` is genuinely absent).
type StreamItem = Capture | StreamRow;

function capOf(item: StreamItem): Capture {
  return "capture" in item ? item.capture : item;
}

function rowOf(item: StreamItem): StreamRow | undefined {
  return "capture" in item ? item : undefined;
}

// Every StreamView-owned mutation handler a CaptureRow in the main stream
// can call (see rowActions below) -- bundled into one object so it can be
// threaded through SortableSectionGroup as a single prop instead of
// nine-plus individually-named ones.
interface CaptureRowActions {
  onPromote: (id: number) => void;
  onDone: (id: number) => void;
  onReopen: (id: number) => void;
  onEdit: (id: number, body: string) => Promise<void>;
  onMerge: (ids: number[]) => void;
  onDelete: (ids: number[]) => void;
  onMoveProject: (ids: number[], projectId: number | null) => void;
  onMoveSection: (ids: number[], sectionId: number | null) => void;
  onCreateSection: (ids: number[], name: string) => void;
  projects: Project[];
  sections: Section[];
}

// Enter/`e` single-key shortcuts open a specific row's Expand modal or
// inline editor from outside that row's own component -- see CaptureRow's
// expandSignal/editSignal props. `token` must change on every trigger
// (even a repeat press on the row that's already showing) so the row's
// effect re-fires; `mode` picks which of the two signals to bump.
interface KeyboardRowTrigger {
  id: number;
  mode: "expand" | "edit";
  token: number;
}

function rowSignals(trigger: KeyboardRowTrigger | null, captureId: number) {
  return {
    expandSignal: trigger?.mode === "expand" && trigger.id === captureId ? trigger.token : undefined,
    editSignal: trigger?.mode === "edit" && trigger.id === captureId ? trigger.token : undefined,
  };
}

interface StreamViewProps {
  stream: StreamRow[];
  sections: Section[];
  projects: Project[];
  refreshStream: () => void;
  refreshNow: () => void;
  refreshSections: () => void;
  showUndoToast: (message: string, onUndo: () => void) => void;
  onPromote: (id: number) => void;
  onDone: (id: number) => void;
  onReopen: (id: number) => void;
  onRenameSection: (id: number, name: string) => void;
  onDeleteSection: (id: number) => void;
  onReorderSection: (id: number, afterId: number | null) => void;
}

type FilterPill = "none" | "project" | "images";

export function StreamView({
  stream,
  sections,
  projects,
  refreshStream,
  refreshNow,
  refreshSections,
  showUndoToast,
  onPromote,
  onDone,
  onReopen,
  onRenameSection,
  onDeleteSection,
  onReorderSection,
}: StreamViewProps) {
  const projectSignal = useProjectSignal();
  const permissionOffer = usePermissionOffer();
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<Capture[] | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [filterPill, setFilterPill] = useState<FilterPill>("none");
  const [unfiledCount, setUnfiledCount] = useState(0);
  const streamContainerRef = useRef<HTMLDivElement>(null);

  const [keyboardTrigger, setKeyboardTrigger] = useState<KeyboardRowTrigger | null>(null);
  const nextKeyboardTriggerToken = useRef(0);
  const triggerRow = useCallback((id: number, mode: "expand" | "edit") => {
    nextKeyboardTriggerToken.current += 1;
    setKeyboardTrigger({ id, mode, token: nextKeyboardTriggerToken.current });
  }, []);

  // Every currently-mounted row registers its own imperative "open my menu"
  // function here, keyed by capture id, so the keyboard handler below can
  // look up whichever row is the current cursor and call straight into the
  // same menuOpenRef the row's own right-click/"..." button already use --
  // not a second menu implementation. A plain ref (not state) because
  // registration churns on every row mount/unmount and none of it needs to
  // trigger a re-render.
  const menuRefs = useRef(new Map<number, (x: number, y: number) => void>());
  const registerMenuOpen = useCallback(
    (id: number, open: ((x: number, y: number) => void) | null) => {
      if (open) menuRefs.current.set(id, open);
      else menuRefs.current.delete(id);
    },
    [],
  );

  const sectionSensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setSearchResults(null);
      return;
    }
    const handle = setTimeout(() => {
      api.searchCaptures(trimmed).then(setSearchResults).catch(console.error);
    }, 150);
    return () => clearTimeout(handle);
  }, [query]);

  // No dedicated "unfiled count changed" event exists -- refetching
  // whenever the stream itself changes is close enough (every path that
  // could change how many captures are unfiled also touches the stream).
  useEffect(() => {
    api.countUnfiled().then(setUnfiledCount).catch(console.error);
  }, [stream]);

  // ⌥U TRIAGE -- scrolls to the first unfiled row rather than opening a
  // dedicated triage surface, which doesn't exist yet (see the redesign
  // plan: unfiled captures already render as normal rows further down the
  // stream, this just gets you there). Local to the stream, not global --
  // it only means something while the stream has focus.
  useEffect(() => {
    function handler(e: KeyboardEvent) {
      if (isTextEntryTarget(e.target)) return;
      if (e.altKey && e.key.toLowerCase() === "u") {
        const firstUnfiled = stream.find((r) => r.capture.project_id === null);
        if (!firstUnfiled) return;
        e.preventDefault();
        document
          .getElementById(`capture-${firstUnfiled.capture.id}`)
          ?.scrollIntoView({ behavior: "smooth", block: "center" });
      }
    }
    const el = streamContainerRef.current;
    el?.addEventListener("keydown", handler);
    return () => el?.removeEventListener("keydown", handler);
  }, [stream]);

  function toggleSelect(id: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  // Delegates to handleMergeIds below rather than duplicating its body --
  // before this fix, this toolbar-only path skipped handleMergeIds's
  // refreshNow()/emit(NOW_CHANGED_EVENT), so merging captures via the
  // toolbar could leave a stale Now sidebar (e.g. when a merged source was
  // promoted) while the identical action via a row's context menu refreshed
  // it correctly. Delegating guarantees the two entry points can't drift
  // apart on this again.
  async function handleMerge() {
    await handleMergeIds(Array.from(selected));
  }

  async function handleCopyAsList() {
    await api.copyCapturesAsChecklist(Array.from(selected));
  }

  async function handleMergeIds(ids: number[]) {
    if (ids.length < 2) return;
    await api.mergeCaptures(ids);
    setSelected(new Set());
    refreshStream();
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleUpdateBody(id: number, body: string) {
    await api.updateCaptureBody(id, body);
    refreshStream();
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleMoveProject(ids: number[], projectId: number | null) {
    await Promise.all(ids.map((id) => api.assignCaptureProject(id, projectId)));
    refreshStream();
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleMoveSection(ids: number[], sectionId: number | null) {
    await Promise.all(ids.map((id) => api.assignCaptureSection(id, sectionId)));
    refreshStream();
    refreshNow();
    emit(SECTIONS_CHANGED_EVENT);
    emit(NOW_CHANGED_EVENT);
  }

  async function handleCreateAndAssignSection(ids: number[], name: string) {
    const section = await api.createSection(name);
    await Promise.all(ids.map((id) => api.assignCaptureSection(id, section.id)));
    refreshSections();
    refreshStream();
    refreshNow();
    emit(SECTIONS_CHANGED_EVENT);
    emit(NOW_CHANGED_EVENT);
  }

  async function handleDeleteCaptures(ids: number[]) {
    await Promise.all(ids.map((id) => api.deleteCapture(id)));
    setSelected((prev) => {
      const next = new Set(prev);
      for (const id of ids) next.delete(id);
      return next;
    });
    refreshStream();
    refreshNow();
    emit(NOW_CHANGED_EVENT);
    showUndoToast(ids.length > 1 ? `${ids.length} captures deleted.` : "Capture deleted.", async () => {
      await Promise.all(ids.map((id) => api.restoreCapture(id)));
      refreshStream();
      refreshNow();
      emit(NOW_CHANGED_EVENT);
    });
  }

  function handleSectionDragEnd(visible: Section[], event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id) return;

    const activeIndex = visible.findIndex((s) => s.id === active.id);
    const overIndex = visible.findIndex((s) => s.id === over.id);
    if (activeIndex === -1 || overIndex === -1) return;

    const afterId =
      activeIndex < overIndex ? visible[overIndex].id : (visible[overIndex - 1]?.id ?? null);
    onReorderSection(Number(active.id), afterId);
  }

  const rowActions: CaptureRowActions = {
    onPromote,
    onDone,
    onReopen,
    onEdit: handleUpdateBody,
    onMerge: handleMergeIds,
    onDelete: handleDeleteCaptures,
    onMoveProject: handleMoveProject,
    onMoveSection: handleMoveSection,
    onCreateSection: handleCreateAndAssignSection,
    projects,
    sections,
  };

  // Filter pills apply only to the non-search view -- search already
  // bypasses grouping entirely (it's the FTS5-ranked flat list), and
  // layering a client-side filter on top of a server-side search would be
  // a second, redundant notion of "what's visible" to keep in sync.
  const filteredStream = useMemo(() => {
    if (filterPill === "project" && projectSignal.projectId != null) {
      return stream.filter((r) => r.capture.project_id === projectSignal.projectId);
    }
    if (filterPill === "images") {
      return stream.filter((r) => r.blob_mime !== null);
    }
    return stream;
  }, [stream, filterPill, projectSignal.projectId]);

  const visibleStream: StreamItem[] = searchResults ?? filteredStream;

  const captureById = useMemo(() => {
    const map = new Map<number, Capture>();
    for (const item of visibleStream) map.set(capOf(item).id, capOf(item));
    return map;
  }, [visibleStream]);

  // useListCursor requires a flat `{id: number}[]` -- StreamRow's id lives
  // at `.capture.id`, so this is the cursor's own visual-order list
  // (Capture[], via capOf), separate from `visibleStream` (StreamItem[])
  // which retains the full StreamRow for rendering/filtering.
  const streamVisualOrder = useMemo(() => {
    if (searchResults) return searchResults;
    const { bySection, unsectioned } = groupBySection(filteredStream, (r) => r.capture.section_id);
    const visibleSections = sections.filter((s) => bySection.has(s.id));
    return [...visibleSections.flatMap((s) => bySection.get(s.id) ?? []), ...unsectioned].map(
      (r) => r.capture,
    );
  }, [searchResults, filteredStream, sections]);

  const streamCursor = useListCursor(streamVisualOrder, {
    onToggleSelect: toggleSelect,
    onExpand: (id) => triggerRow(id, "expand"),
    onAction: (key, id) => {
      const capture = captureById.get(id);
      const ids = selected.size > 0 ? Array.from(selected) : [id];
      if (key === "d") {
        if (capture?.done_at) onReopen(id);
        else onDone(id);
      } else if (key === "c") {
        if (capture?.body === "") api.copyCaptureImage(id);
        else api.copyCaptureText(id);
      } else if (key === "C") {
        api.copyCapturesAsChecklist(ids);
      } else if (key === "e") {
        triggerRow(id, "edit");
      } else if (key === "M") {
        handleMergeIds(ids);
      } else if (key === "Backspace" || key === "Delete") {
        handleDeleteCaptures(ids);
      }
    },
    onOpenMenu: (id) => {
      const open = menuRefs.current.get(id);
      const rect = document.getElementById(`capture-${id}`)?.getBoundingClientRect();
      if (!open || !rect) return;
      open(rect.right - 32, rect.top + 8);
    },
  });

  function renderRow(item: StreamItem) {
    const capture = capOf(item);
    const streamRow = rowOf(item);
    return (
      <CaptureRow
        key={capture.id}
        capture={capture}
        streamRow={streamRow}
        selected={selected.has(capture.id)}
        selectedIds={selected}
        onToggleSelect={toggleSelect}
        onPromote={rowActions.onPromote}
        onDone={rowActions.onDone}
        onReopen={rowActions.onReopen}
        onEdit={rowActions.onEdit}
        onMerge={rowActions.onMerge}
        onDelete={rowActions.onDelete}
        onMoveProject={rowActions.onMoveProject}
        onMoveSection={rowActions.onMoveSection}
        onCreateSection={rowActions.onCreateSection}
        projects={rowActions.projects}
        sections={rowActions.sections}
        {...rowSignals(keyboardTrigger, capture.id)}
        isCursor={capture.id === streamCursor.cursorId}
        onRegisterMenuOpen={registerMenuOpen}
      />
    );
  }

  return (
    <div
      ref={streamContainerRef}
      tabIndex={-1}
      className="flex flex-1 flex-col gap-3 overflow-hidden p-3"
    >
      <div className="flex items-center gap-2">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search captures…"
          className="flex-1 rounded-md border border-hairline bg-transparent px-3 py-1.5 text-sm text-fg focus:border-accent-line focus:outline-none"
        />
        <Earned when={projectSignal.projectId != null}>
          <Chip
            variant={filterPill === "project" ? "solid" : "neutral"}
            onClick={() => setFilterPill((p) => (p === "project" ? "none" : "project"))}
          >
            THIS PROJECT
          </Chip>
        </Earned>
        <Chip
          variant={filterPill === "images" ? "solid" : "neutral"}
          onClick={() => setFilterPill((p) => (p === "images" ? "none" : "images"))}
        >
          IMAGES
        </Chip>
      </div>

      <MergeToolbar
        count={selected.size}
        onMerge={handleMerge}
        onCopyAsList={handleCopyAsList}
        onMoveToProject={(projectId) => handleMoveProject(Array.from(selected), projectId)}
        onMoveToSection={(sectionId) => handleMoveSection(Array.from(selected), sectionId)}
        onDelete={() => handleDeleteCaptures(Array.from(selected))}
        onClear={() => setSelected(new Set())}
        projects={projects}
        sections={sections}
      />

      <div
        onKeyDown={streamCursor.onKeyDown}
        className="flex-1 overflow-y-auto focus:outline-none focus-within:ring-1 focus-within:ring-accent-line"
      >
        {/* The permission upgrade offer, first slot in the stream -- ahead
            of Unfiled, since it's about the capture mechanism itself
            rather than anything already captured. Earned at 24
            clipboard-only captures; see usePermissionOffer. */}
        <Earned when={!searchResults && permissionOffer.showCard}>
          <PermissionCard onUpgrade={permissionOffer.upgrade} onDismiss={permissionOffer.dismiss} />
        </Earned>

        {/* Unfiled pins to the top as a lane, not a folder -- the unfiled
            captures themselves still render as normal rows further down
            (this is a summary/entry point, not a filter). Earned: absent
            entirely at zero, per the redesign plan -- an empty lane reads
            as broken, no lane reads as "nothing to file." */}
        <Earned when={!searchResults && unfiledCount > 0}>
          <div className="mb-2 flex items-center gap-3 rounded-md border border-accent-line bg-accent-tint px-3 py-2.5">
            <Mono size="sm" tone="accent" weight="bold">
              UNFILED · {unfiledCount}
            </Mono>
            <span className="flex-1 text-xs text-fg-muted">
              Captured with no repo in sight. File them or leave them — they're safe either way.
            </span>
            <Mono size="xs" tone="faint">
              ⌥U TRIAGE
            </Mono>
          </div>
        </Earned>

        {visibleStream.length === 0 ? (
          <p className="px-3 py-6 text-center text-sm text-neutral-400 dark:text-neutral-600">
            {searchResults ? "No matches." : "Nothing captured yet — try the hotkey."}
          </p>
        ) : searchResults ? (
          // Search ignores section grouping entirely -- it's the FTS5-ranked
          // flat list, exactly as it arrives.
          <div className="flex flex-col gap-2">{searchResults.map(renderRow)}</div>
        ) : (
          (() => {
            const { bySection, unsectioned } = groupBySection(
              filteredStream,
              (r) => r.capture.section_id,
            );
            const visibleSections = sections.filter((s) => bySection.has(s.id));
            return (
              <div className="flex flex-col gap-2">
                {/* Only mounted once a section actually has members -- with
                    no sections created, this whole block is absent, not
                    merely empty. */}
                {visibleSections.length > 0 && (
                  <DndContext
                    sensors={sectionSensors}
                    collisionDetection={closestCenter}
                    onDragEnd={(event) => handleSectionDragEnd(visibleSections, event)}
                  >
                    <SortableContext
                      items={visibleSections.map((s) => s.id)}
                      strategy={verticalListSortingStrategy}
                    >
                      {visibleSections.map((s) => (
                        <SortableSectionGroup
                          key={s.id}
                          section={s}
                          rows={bySection.get(s.id)!}
                          onRenameSection={onRenameSection}
                          onDeleteSection={onDeleteSection}
                          renderRow={renderRow}
                        />
                      ))}
                    </SortableContext>
                  </DndContext>
                )}
                {/* Unsectioned items render in their existing order with no
                    header at all -- if you've never created a section, this
                    branch is the entirety of the list. */}
                {unsectioned.map(renderRow)}
              </div>
            );
          })()
        )}
      </div>
    </div>
  );
}

// The section-group analog of NowList's `SortableCaptureRow`: the entire
// header+members block is the draggable unit (so reordering a section
// carries its rendered captures along with it), but only the header's grip
// button is wired as the drag handle. `renderRow` is threaded through
// rather than duplicating the row-rendering call site a third time.
function SortableSectionGroup({
  section,
  rows,
  onRenameSection,
  onDeleteSection,
  renderRow,
}: {
  section: Section;
  rows: StreamItem[];
  onRenameSection: (id: number, name: string) => void;
  onDeleteSection: (id: number) => void;
  renderRow: (item: StreamItem) => React.ReactNode;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: section.id,
  });

  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={isDragging ? "opacity-50" : undefined}
    >
      <SectionHeader
        section={section}
        onRename={(name) => onRenameSection(section.id, name)}
        onDelete={() => onDeleteSection(section.id)}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
      <div className="mt-2 flex flex-col gap-2">{rows.map(renderRow)}</div>
    </div>
  );
}
