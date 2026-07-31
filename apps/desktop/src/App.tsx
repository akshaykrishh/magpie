import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import { SortableContext, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { emit, listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AddPromptInput } from "./components/AddPromptInput";
import { AuditView } from "./components/AuditView";
import { CaptureItem } from "./components/CaptureItem";
import { Logo, Wordmark } from "./components/Logo";
import { MergeToolbar } from "./components/MergeToolbar";
import { NowList } from "./components/NowList";
import { PermissionBanner } from "./components/PermissionBanner";
import { RecentlyDeletedView } from "./components/RecentlyDeletedView";
import { SearchBar } from "./components/SearchBar";
import { SectionHeader } from "./components/SectionHeader";
import { TemplatesPanel } from "./components/TemplatesPanel";
import { UndoToast } from "./components/UndoToast";
import { api } from "./lib/api";
import { NOW_CHANGED_EVENT, SECTIONS_CHANGED_EVENT } from "./lib/events";
import type { Capabilities, Capture, Project, Section } from "./lib/types";
import { useListCursor } from "./lib/useListCursor";
import { cn } from "./lib/utils";

// Groups items that carry a `section_id` into per-section buckets (in the
// order the items themselves already arrive in) plus a leftover bucket for
// items with no section -- shared shape used by both the main stream and
// NowList so "grouped by section, unsectioned items last, no dimension
// added to their existing order" means the same thing in both places.
function groupBySection<T extends { section_id: number | null }>(items: T[]) {
  const bySection = new Map<number, T[]>();
  const unsectioned: T[] = [];
  for (const item of items) {
    if (item.section_id === null) unsectioned.push(item);
    else bySection.set(item.section_id, [...(bySection.get(item.section_id) ?? []), item]);
  }
  return { bySection, unsectioned };
}

// Every App-owned mutation handler a CaptureItem in the main stream can
// call (see rowActions in App() below) -- bundled into one object so it can
// be threaded through SortableSectionGroup as a single prop instead of
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

// Task 24's Enter/`e` single-key shortcuts open a specific row's Expand
// modal or inline editor from outside that row's own component -- see
// CaptureItem's expandSignal/editSignal props. `token` must change on every
// trigger (even a repeat press on the row that's already showing) so the
// row's effect re-fires; `mode` picks which of the two signals to bump.
interface KeyboardRowTrigger {
  id: number;
  mode: "expand" | "edit";
  token: number;
}

// Resolves the two per-row signal props for a single CaptureItem from the
// shared trigger state -- every render site below calls this the same way,
// so a row only ever receives a defined signal when it's actually the row
// the keyboard shortcut targeted.
function rowSignals(trigger: KeyboardRowTrigger | null, captureId: number) {
  return {
    expandSignal: trigger?.mode === "expand" && trigger.id === captureId ? trigger.token : undefined,
    editSignal: trigger?.mode === "edit" && trigger.id === captureId ? trigger.token : undefined,
  };
}

// The section-group analog of NowList's `SortableCaptureItem`: the entire
// header+members block is the draggable unit (so reordering a section
// carries its rendered captures along with it), but only the header's grip
// button is wired as the drag handle.
function SortableSectionGroup({
  section,
  captures,
  selected,
  onToggleSelect,
  onRenameSection,
  onDeleteSection,
  rowActions,
  keyboardTrigger,
  cursorId,
}: {
  section: Section;
  captures: Capture[];
  selected: Set<number>;
  onToggleSelect: (id: number) => void;
  onRenameSection: (id: number, name: string) => void;
  onDeleteSection: (id: number) => void;
  rowActions: CaptureRowActions;
  keyboardTrigger: KeyboardRowTrigger | null;
  cursorId: number | null;
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
      <div className="mt-2 flex flex-col gap-2">
        {captures.map((capture) => (
          <CaptureItem
            key={capture.id}
            capture={capture}
            selected={selected.has(capture.id)}
            selectedIds={selected}
            onToggleSelect={onToggleSelect}
            onPromote={capture.queue_pos === null ? rowActions.onPromote : undefined}
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
            isCursor={capture.id === cursorId}
          />
        ))}
      </div>
    </div>
  );
}

// Never on first run -- only once the user has felt the two-keystroke
// friction a few times is the upgrade worth interrupting them for.
const CAPTURES_BEFORE_UPGRADE_OFFER = 3;

type View = "captures" | "templates" | "activity" | "recently_deleted";
const VIEWS: { id: View; label: string }[] = [
  { id: "captures", label: "Captures" },
  { id: "templates", label: "Templates" },
  { id: "activity", label: "Activity" },
  { id: "recently_deleted", label: "Recently Deleted" },
];

function App() {
  const [view, setView] = useState<View>("captures");
  const [stream, setStream] = useState<Capture[]>([]);
  const [now, setNow] = useState<Capture[]>([]);
  const [sections, setSections] = useState<Section[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<Capture[] | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const [permissionBannerDismissed, setPermissionBannerDismissed] = useState(false);
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

  // Task 24's Enter/`e` list shortcuts -- see KeyboardRowTrigger's comment
  // above. `nextKeyboardTriggerToken` guarantees a fresh token even for a
  // repeat press on the same row (e.g. Enter, close the modal, Enter
  // again), the same "identify this firing, not just the target" pattern
  // `nextUndoToastId` already uses for UndoToast above.
  const [keyboardTrigger, setKeyboardTrigger] = useState<KeyboardRowTrigger | null>(null);
  const nextKeyboardTriggerToken = useRef(0);
  const triggerRow = useCallback((id: number, mode: "expand" | "edit") => {
    nextKeyboardTriggerToken.current += 1;
    setKeyboardTrigger({ id, mode, token: nextKeyboardTriggerToken.current });
  }, []);

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
    api.listStream({ kind: "all" }).then(setStream).catch(console.error);
  }, []);

  const refreshNow = useCallback(() => {
    api.listNow(null).then(setNow).catch(console.error);
  }, []);

  const refreshSections = useCallback(() => {
    api.listSections().then(setSections).catch(console.error);
  }, []);

  const refreshProjects = useCallback(() => {
    api.listProjects().then(setProjects).catch(console.error);
  }, []);

  const sectionSensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  useEffect(() => {
    refreshStream();
    refreshNow();
    refreshSections();
    refreshProjects();
    api.captureCapabilities().then(setCapabilities).catch(console.error);

    const unlistenCapture = listen("capture:added", () => {
      refreshStream();
      // Capability state itself doesn't change per-capture, but this is
      // cheap and keeps it correct if the user grants Accessibility while
      // the app is running (macOS doesn't require a relaunch for the
      // AXIsProcessTrusted check to reflect a new grant).
      api.captureCapabilities().then(setCapabilities).catch(console.error);
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
    const unlistenNow = listen(NOW_CHANGED_EVENT, refreshNow);
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
      unlistenCapture.then((f) => f());
      unlistenCaptureUpdated.then((f) => f());
      unlistenNow.then((f) => f());
      unlistenSections.then((f) => f());
    };
  }, [refreshStream, refreshNow, refreshSections, refreshProjects]);

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

  function toggleSelect(id: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

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

  // Toolbar-only action: the row context menu's "Copy as List" calls
  // api.copyCapturesAsChecklist(ids) directly with no App-owned handler in
  // between (see CaptureItem's buildContextMenuItems) -- this thin wrapper
  // makes the exact same call the exact same way (same ids ordering, via
  // Array.from(selected), matching targetIds()'s Array.from(selectedIds))
  // so the two entry points produce byte-identical clipboard output for the
  // same selection.
  async function handleCopyAsList() {
    await api.copyCapturesAsChecklist(Array.from(selected));
  }

  // Context-menu counterpart of handleMerge above: MergeToolbar always acts
  // on `selected` (the batch bar itself is the selection UI), but a row's
  // "Merge Notes" item works off whatever CaptureItem's own batch-aware rule
  // resolved (the checked selection if non-empty, else just that row) --
  // ids.length < 2 is already guarded against by the menu item's own
  // `disabled` (merge_captures rejects fewer than 2 sources), so this only
  // has to guard the same invariant defensively.
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

  // Batch-aware delete: ids is either the checked selection or the single
  // clicked row (CaptureItem's targetIds()). Each delete is soft (Task 4/8),
  // so Undo just restores every id this call touched -- one toast covers the
  // whole batch rather than one per row.
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

  function handleSectionDragEnd(visible: Section[], event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id) return;

    const activeIndex = visible.findIndex((s) => s.id === active.id);
    const overIndex = visible.findIndex((s) => s.id === over.id);
    if (activeIndex === -1 || overIndex === -1) return;

    const afterId =
      activeIndex < overIndex ? visible[overIndex].id : (visible[overIndex - 1]?.id ?? null);
    handleReorderSection(Number(active.id), afterId);
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

  // The full set of App-owned mutation handlers every CaptureItem in the
  // main stream needs (Now-list rows don't get these -- see CaptureItem's
  // props, all optional, for what that degrades to). One object so each of
  // the three render sites below (search results, sectioned, unsectioned)
  // stays in sync rather than drifting prop-by-prop.
  const rowActions: CaptureRowActions = {
    onPromote: handlePromote,
    onDone: handleDone,
    onReopen: handleReopen,
    onEdit: handleUpdateBody,
    onMerge: handleMergeIds,
    onDelete: handleDeleteCaptures,
    onMoveProject: handleMoveProject,
    onMoveSection: handleMoveSection,
    onCreateSection: handleCreateAndAssignSection,
    projects,
    sections,
  };

  const visibleStream = searchResults ?? stream;

  // Task 24's single-key action dispatch below needs each action's own
  // underlying capture (done_at for the "d" done/reopen toggle, body for the
  // "c" copy-image-vs-text branch) by id -- covers whichever list is
  // actually visible (search results or the plain stream), same as
  // `visibleStream` itself.
  const captureById = useMemo(() => {
    const map = new Map<number, Capture>();
    for (const c of visibleStream) map.set(c.id, c);
    return map;
  }, [visibleStream]);

  // The stream cursor must move in the same order the rows actually render
  // in, not `visibleStream`'s raw order -- once any section has members, the
  // non-search render below groups all of that section's captures together
  // (in `sections` order) before the unsectioned leftovers, which can differ
  // from `stream`'s own interleaving. Search results bypass grouping
  // entirely (rendered flat, exactly as `searchResults` arrives), so this
  // only reshapes the non-search case. Built from the same `groupBySection`
  // helper and the same `sections` filter the render below uses, so the two
  // can't drift apart on what "visible" means.
  const streamVisualOrder = useMemo(() => {
    if (searchResults) return searchResults;
    const { bySection, unsectioned } = groupBySection(stream);
    const visibleSections = sections.filter((s) => bySection.has(s.id));
    return [...visibleSections.flatMap((s) => bySection.get(s.id) ?? []), ...unsectioned];
  }, [searchResults, stream, sections]);
  // Single-key action dispatch below mirrors the context menu's own
  // per-action batching exactly (see CaptureItem's buildContextMenuItems):
  // Copy as List / Merge / Delete already act on "the checked selection if
  // non-empty, else just this row" there, so "C"/"M"/Backspace-Delete do the
  // same here via `ids`. Mark Done, Copy, Edit, and Expand are NOT
  // batch-aware anywhere else in the app today (their menu items always
  // close over `capture.id`, never `targetIds()`) -- so "d"/"c"/"e"/Enter
  // stay single-row here too, rather than inventing new batch behavior for
  // an action that ignores the selection everywhere else it's reachable.
  const streamCursor = useListCursor(streamVisualOrder, {
    onToggleSelect: toggleSelect,
    onExpand: (id) => triggerRow(id, "expand"),
    onAction: (key, id) => {
      const capture = captureById.get(id);
      const ids = selected.size > 0 ? Array.from(selected) : [id];
      if (key === "d") {
        // Mirrors the menu's Reopen/Mark Done toggle exactly (see
        // buildContextMenuItems: `capture.done_at ? Reopen : Mark Done`).
        if (capture?.done_at) handleReopen(id);
        else handleDone(id);
      } else if (key === "c") {
        // Mirrors the menu's plain "Copy" item, including its
        // screenshot-vs-text branch.
        if (capture?.body === "") api.copyCaptureImage(id);
        else api.copyCaptureText(id);
      } else if (key === "C") {
        api.copyCapturesAsChecklist(ids);
      } else if (key === "e") {
        // CaptureItem's own editSignal effect ignores this for
        // screenshots/session digests, matching editActionDisabled.
        triggerRow(id, "edit");
      } else if (key === "M") {
        // handleMergeIds already no-ops below 2 ids (merge_captures itself
        // rejects it), so no extra guard is needed here.
        handleMergeIds(ids);
      } else if (key === "Backspace" || key === "Delete") {
        handleDeleteCaptures(ids);
      }
    },
  });

  const showPermissionBanner =
    !permissionBannerDismissed &&
    capabilities?.mode === "clipboard_only" &&
    capabilities.synthesized_copy_available &&
    stream.length >= CAPTURES_BEFORE_UPGRADE_OFFER;

  return (
    <main className="flex h-screen flex-col overflow-hidden">
      <header className="flex shrink-0 items-center gap-1.5 border-b border-neutral-200 px-3 py-2 dark:border-neutral-800">
        <Logo size={18} />
        <Wordmark className="text-sm font-bold text-ink dark:text-neutral-100" />
      </header>
      {showPermissionBanner && (
        <div className="p-3 pb-0">
          <PermissionBanner
            onUpgrade={() => {
              api.openAccessibilitySettings().catch(console.error);
            }}
            onDismiss={() => setPermissionBannerDismissed(true)}
          />
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
        <aside className="flex w-80 shrink-0 flex-col gap-3 border-r border-neutral-200 p-3 dark:border-neutral-800">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-400">
            Now
          </h2>
          <AddPromptInput onAdd={handleAddPrompt} />
          <div className="flex-1 overflow-y-auto">
            <NowList
              items={now}
              onReorder={handleNowReorder}
              onDone={handleDone}
              onDemote={handleDemote}
              sections={sections}
              onRenameSection={handleRenameSection}
              onDeleteSection={handleDeleteSection}
              onReorderSection={handleReorderSection}
            />
          </div>
        </aside>

        <section className="flex flex-1 flex-col overflow-hidden">
          <nav className="flex gap-1 border-b border-neutral-200 px-3 pt-3 dark:border-neutral-800">
            {VIEWS.map((v) => (
              <button
                key={v.id}
                type="button"
                onClick={() => setView(v.id)}
                className={cn(
                  "rounded-t-md px-3 py-1.5 text-sm",
                  view === v.id
                    ? "border-b-2 border-slate-teal font-medium text-slate-teal dark:border-slate-teal-light dark:text-slate-teal-light"
                    : "text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200",
                )}
              >
                {v.label}
              </button>
            ))}
          </nav>

          {view === "captures" && (
            <div className="flex flex-1 flex-col gap-3 overflow-hidden p-3">
              <SearchBar value={query} onChange={setQuery} />
              <MergeToolbar
                count={selected.size}
                onMerge={handleMerge}
                onCopyAsList={handleCopyAsList}
                // Same handlers rowActions already threads into every
                // CaptureItem's context menu (handleMoveProject/handleMoveSection/
                // handleDeleteCaptures all take an ids array) -- the toolbar just
                // supplies Array.from(selected) as that array, identical to what
                // CaptureItem's own targetIds() resolves to for a non-empty
                // selection, so the same click produces the same mutation, the
                // same refetches, and (for delete) the same Undo toast whichever
                // entry point triggered it.
                onMoveToProject={(projectId) => handleMoveProject(Array.from(selected), projectId)}
                onMoveToSection={(sectionId) => handleMoveSection(Array.from(selected), sectionId)}
                onDelete={() => handleDeleteCaptures(Array.from(selected))}
                onClear={() => setSelected(new Set())}
                projects={projects}
                sections={sections}
              />
              <div
                tabIndex={0}
                onKeyDown={streamCursor.onKeyDown}
                className="flex-1 overflow-y-auto focus:outline-none focus-within:ring-1 focus-within:ring-slate-teal/30"
              >
                {visibleStream.length === 0 ? (
                  <p className="px-3 py-6 text-center text-sm text-neutral-400 dark:text-neutral-600">
                    {searchResults ? "No matches." : "Nothing captured yet — try the hotkey."}
                  </p>
                ) : searchResults ? (
                  // Search ignores section grouping entirely -- it's the existing
                  // FTS5-ranked flat list, exactly as before this task.
                  <div className="flex flex-col gap-2">
                    {searchResults.map((capture) => (
                      <CaptureItem
                        key={capture.id}
                        capture={capture}
                        selected={selected.has(capture.id)}
                        selectedIds={selected}
                        onToggleSelect={toggleSelect}
                        onPromote={capture.queue_pos === null ? rowActions.onPromote : undefined}
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
                      />
                    ))}
                  </div>
                ) : (
                  (() => {
                    const { bySection, unsectioned } = groupBySection(stream);
                    const visibleSections = sections.filter((s) => bySection.has(s.id));
                    return (
                      <div className="flex flex-col gap-2">
                        {/* Only mounted once a section actually has members --
                            with no sections created, this whole block is
                            absent, not merely empty, so the zero-sections
                            render carries none of this task's DnD scaffolding
                            at all (matches the pre-Task-19 output exactly). */}
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
                                  captures={bySection.get(s.id)!}
                                  selected={selected}
                                  onToggleSelect={toggleSelect}
                                  onRenameSection={handleRenameSection}
                                  onDeleteSection={handleDeleteSection}
                                  rowActions={rowActions}
                                  keyboardTrigger={keyboardTrigger}
                                  cursorId={streamCursor.cursorId}
                                />
                              ))}
                            </SortableContext>
                          </DndContext>
                        )}
                        {/* Unsectioned items render in their existing order with no
                            header at all -- if you've never created a section, this
                            branch is the entirety of the list, byte-identical to the
                            pre-Task-19 flat render below. */}
                        {unsectioned.map((capture) => (
                          <CaptureItem
                            key={capture.id}
                            capture={capture}
                            selected={selected.has(capture.id)}
                            selectedIds={selected}
                            onToggleSelect={toggleSelect}
                            onPromote={capture.queue_pos === null ? rowActions.onPromote : undefined}
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
                          />
                        ))}
                      </div>
                    );
                  })()
                )}
              </div>
            </div>
          )}

          {view === "templates" && (
            <TemplatesPanel
              onInstantiated={() => {
                refreshNow();
                emit(NOW_CHANGED_EVENT);
              }}
              onShowUndo={showUndoToast}
            />
          )}

          {view === "activity" && <AuditView />}

          {view === "recently_deleted" && <RecentlyDeletedView />}
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
    </main>
  );
}

export default App;
