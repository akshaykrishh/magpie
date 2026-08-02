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
import { afterIdFromDragEnd, groupBySection } from "@/lib/grouping";
import { elapsed, nowRowState, sessionLabel } from "@/lib/format";
import type { Capture, Section, Session } from "@/lib/types";
import { useListCursor } from "@/lib/useListCursor";
import { CaptureRow } from "./CaptureRow";
import { SectionHeader } from "./SectionHeader";

interface NowListProps {
  items: Capture[];
  onReorder: (id: number, afterId: number | null) => void;
  onDone: (id: number) => void;
  onDemote: (id: number) => void;
  sections: Section[];
  onRenameSection: (id: number, name: string) => void;
  onDeleteSection: (id: number) => void;
  onReorderSection: (id: number, afterId: number | null) => void;
  /** For the "LEASED BY S1 · 12M" chip -- looked up by `capture.lease_session`.
      NowList doesn't fetch this itself (see NowColumn, which owns the
      fetch and re-fetches whenever `items` changes) so a caller with no
      session data at all (there isn't one yet) can just pass `[]`. */
  sessions?: Session[];
  onRevokeLease?: (id: number) => void;
  onOpenReview?: (capture: Capture) => void;
}

/// "LEASED BY S1 · 12M" -- looked up from the session holding the lease,
/// elapsed since `lease_at`. `undefined` when the lease_session doesn't
/// match any known session (a narrow race: the session ended between this
/// list's own refetch and the sessions fetch) -- the chip simply doesn't
/// render rather than showing a broken label.
function leaseLabelFor(capture: Capture, sessions: Session[]): string | undefined {
  if (!capture.lease_session || !capture.lease_at) return undefined;
  const session = sessions.find((s) => s.id === capture.lease_session);
  if (!session) return undefined;
  return `LEASED BY ${sessionLabel(session)} · ${elapsed(capture.lease_at)}`;
}

export function NowList({
  items,
  onReorder,
  onDone,
  onDemote,
  sections,
  onRenameSection,
  onDeleteSection,
  onReorderSection,
  sessions = [],
  onRevokeLease,
  onOpenReview,
}: NowListProps) {
  const sectionSensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const unsectionedSensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  // Computed unconditionally (ahead of the empty-state early return below)
  // so the useListCursor() call after it never gets skipped on some renders
  // and not others -- that would violate the rules of hooks the moment
  // `items` goes from empty to non-empty or back.
  const { bySection, unsectioned } = groupBySection(items, (c) => c.section_id);
  const visibleSections = sections.filter((s) => bySection.has(s.id));
  // Cursor must move in the same order these rows actually render in: all of
  // a section's members together (in `sections` order), then the
  // unsectioned leftovers -- not `items`'s own raw order, which can
  // interleave the two. See the identical comment on App.tsx's
  // `streamVisualOrder`.
  const visualOrder = [...visibleSections.flatMap((s) => bySection.get(s.id) ?? []), ...unsectioned];
  const cursor = useListCursor(visualOrder);

  if (items.length === 0) {
    return (
      <div
        tabIndex={0}
        onKeyDown={cursor.onKeyDown}
        className="focus:outline-none focus-within:ring-1 focus-within:ring-accent-line"
      >
        <p className="px-3 py-6 text-center text-sm text-neutral-400 dark:text-neutral-600">
          Nothing queued. Promote a capture from the stream, or type a prompt to add one directly.
        </p>
      </div>
    );
  }

  function handleSectionDragEnd(event: DragEndEvent) {
    const afterId = afterIdFromDragEnd(visibleSections, event);
    if (afterId === undefined) return;
    onReorderSection(Number(event.active.id), afterId);
  }

  function handleUnsectionedDragEnd(event: DragEndEvent) {
    const afterId = afterIdFromDragEnd(unsectioned, event);
    if (afterId === undefined) return;
    onReorder(Number(event.active.id), afterId);
  }

  return (
    <div
      tabIndex={0}
      onKeyDown={cursor.onKeyDown}
      className="flex flex-col gap-2 focus:outline-none focus-within:ring-1 focus-within:ring-accent-line"
    >
      {/* Only mounted once a section actually has members -- with no
          sections created, this whole block is absent, not merely empty,
          so the zero-sections render carries none of this task's DnD
          scaffolding at all (matches the pre-Task-19 output exactly). */}
      {visibleSections.length > 0 && (
        <DndContext
          sensors={sectionSensors}
          collisionDetection={closestCenter}
          onDragEnd={handleSectionDragEnd}
        >
          <SortableContext items={visibleSections.map((s) => s.id)} strategy={verticalListSortingStrategy}>
            {visibleSections.map((s) => (
              <SortableSectionGroup
                key={s.id}
                section={s}
                captures={bySection.get(s.id)!}
                onDone={onDone}
                onDemote={onDemote}
                onReorder={onReorder}
                onRename={onRenameSection}
                onDelete={onDeleteSection}
                cursorId={cursor.cursorId}
                sessions={sessions}
                onRevokeLease={onRevokeLease}
                onOpenReview={onOpenReview}
              />
            ))}
          </SortableContext>
        </DndContext>
      )}

      <DndContext
        sensors={unsectionedSensors}
        collisionDetection={closestCenter}
        onDragEnd={handleUnsectionedDragEnd}
      >
        <SortableContext items={unsectioned.map((c) => c.id)} strategy={verticalListSortingStrategy}>
          {unsectioned.map((capture) => (
            <SortableCaptureRow
              key={capture.id}
              capture={capture}
              onDone={onDone}
              onDemote={onDemote}
              isCursor={capture.id === cursor.cursorId}
              sessions={sessions}
              onRevokeLease={onRevokeLease}
              onOpenReview={onOpenReview}
            />
          ))}
        </SortableContext>
      </DndContext>
    </div>
  );
}

// A section header plus its own captures, all as one draggable unit for
// section-header reordering (the whole block moves together), with its
// captures given their own independent, local drag-to-reorder scope --
// exactly the pre-Task-19 capture reordering, just applied to this group's
// slice of `items` rather than the full flat list.
function SortableSectionGroup({
  section,
  captures,
  onDone,
  onDemote,
  onReorder,
  onRename,
  onDelete,
  cursorId,
  sessions,
  onRevokeLease,
  onOpenReview,
}: {
  section: Section;
  captures: Capture[];
  onDone: (id: number) => void;
  onDemote: (id: number) => void;
  onReorder: (id: number, afterId: number | null) => void;
  onRename: (id: number, name: string) => void;
  onDelete: (id: number) => void;
  cursorId: number | null;
  sessions: Session[];
  onRevokeLease?: (id: number) => void;
  onOpenReview?: (capture: Capture) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: section.id,
  });
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  function handleDragEnd(event: DragEndEvent) {
    const afterId = afterIdFromDragEnd(captures, event);
    if (afterId === undefined) return;
    onReorder(Number(event.active.id), afterId);
  }

  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={isDragging ? "opacity-50" : undefined}
    >
      <SectionHeader
        section={section}
        onRename={(name) => onRename(section.id, name)}
        onDelete={() => onDelete(section.id)}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext items={captures.map((c) => c.id)} strategy={verticalListSortingStrategy}>
          <div className="mt-2 flex flex-col gap-2">
            {captures.map((capture) => (
              <SortableCaptureRow
                key={capture.id}
                capture={capture}
                onDone={onDone}
                onDemote={onDemote}
                isCursor={capture.id === cursorId}
                sessions={sessions}
                onRevokeLease={onRevokeLease}
                onOpenReview={onOpenReview}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>
    </div>
  );
}

function SortableCaptureRow({
  capture,
  onDone,
  onDemote,
  isCursor,
  sessions,
  onRevokeLease,
  onOpenReview,
}: {
  capture: Capture;
  onDone: (id: number) => void;
  onDemote: (id: number) => void;
  isCursor?: boolean;
  sessions: Session[];
  onRevokeLease?: (id: number) => void;
  onOpenReview?: (capture: Capture) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: capture.id,
  });

  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={isDragging ? "opacity-50" : undefined}
    >
      <CaptureRow
        capture={capture}
        onDone={onDone}
        onDemote={onDemote}
        dragHandleProps={{ ...attributes, ...listeners }}
        isCursor={isCursor}
        nowState={nowRowState(capture)}
        leaseLabel={leaseLabelFor(capture, sessions)}
        onRevokeLease={onRevokeLease ? () => onRevokeLease(capture.id) : undefined}
        onOpenReview={onOpenReview ? () => onOpenReview(capture) : undefined}
      />
    </div>
  );
}
