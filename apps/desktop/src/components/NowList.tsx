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
import type { Capture, Section } from "@/lib/types";
import { useListCursor } from "@/lib/useListCursor";
import { CaptureItem } from "./CaptureItem";
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
}

// Groups items that carry a `section_id` into per-section buckets (in the
// order the items themselves already arrive in, i.e. still `queue_pos`
// order -- grouping never introduces a second ordering dimension) plus a
// leftover bucket for items with no section. Mirrors the identically-named
// helper in App.tsx; kept local here since NowList and App.tsx aren't
// otherwise coupled.
function groupBySection<T extends { section_id: number | null }>(items: T[]) {
  const bySection = new Map<number, T[]>();
  const unsectioned: T[] = [];
  for (const item of items) {
    if (item.section_id === null) unsectioned.push(item);
    else bySection.set(item.section_id, [...(bySection.get(item.section_id) ?? []), item]);
  }
  return { bySection, unsectioned };
}

// Shared by every drag-to-reorder list below (a section's own captures, the
// unsectioned leftovers, and the section headers themselves) -- computes
// "the id this element should now sit immediately after" from a plain
// DragEndEvent plus the *local* ordered array that list is rendering.
// `undefined` means "no-op" (dropped on itself, or off-target). This is
// exactly the pre-Task-19 `handleDragEnd` body, just factored out so each
// group can call it against its own slice without introducing any new
// ordering math.
function afterIdFromDragEnd<T extends { id: number }>(
  localOrder: T[],
  event: DragEndEvent,
): number | null | undefined {
  const { active, over } = event;
  if (!over || active.id === over.id) return undefined;

  const activeIndex = localOrder.findIndex((c) => c.id === active.id);
  const overIndex = localOrder.findIndex((c) => c.id === over.id);
  if (activeIndex === -1 || overIndex === -1) return undefined;

  return activeIndex < overIndex ? localOrder[overIndex].id : (localOrder[overIndex - 1]?.id ?? null);
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
  const { bySection, unsectioned } = groupBySection(items);
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
        className="focus:outline-none focus-within:ring-1 focus-within:ring-slate-teal/30"
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
      className="flex flex-col gap-2 focus:outline-none focus-within:ring-1 focus-within:ring-slate-teal/30"
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
            <SortableCaptureItem key={capture.id} capture={capture} onDone={onDone} onDemote={onDemote} />
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
}: {
  section: Section;
  captures: Capture[];
  onDone: (id: number) => void;
  onDemote: (id: number) => void;
  onReorder: (id: number, afterId: number | null) => void;
  onRename: (id: number, name: string) => void;
  onDelete: (id: number) => void;
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
              <SortableCaptureItem
                key={capture.id}
                capture={capture}
                onDone={onDone}
                onDemote={onDemote}
              />
            ))}
          </div>
        </SortableContext>
      </DndContext>
    </div>
  );
}

function SortableCaptureItem({
  capture,
  onDone,
  onDemote,
}: {
  capture: Capture;
  onDone: (id: number) => void;
  onDemote: (id: number) => void;
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
      <CaptureItem
        capture={capture}
        onDone={onDone}
        onDemote={onDemote}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
}
