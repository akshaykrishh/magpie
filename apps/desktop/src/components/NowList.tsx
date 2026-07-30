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
import type { Capture } from "@/lib/types";
import { CaptureItem } from "./CaptureItem";

interface NowListProps {
  items: Capture[];
  onReorder: (id: number, afterId: number | null) => void;
  onDone: (id: number) => void;
  onDemote: (id: number) => void;
}

export function NowList({ items, onReorder, onDone, onDemote }: NowListProps) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  function handleDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id) return;

    const activeIndex = items.findIndex((c) => c.id === active.id);
    const overIndex = items.findIndex((c) => c.id === over.id);
    if (activeIndex === -1 || overIndex === -1) return;

    // The capture ends up immediately after whichever item currently sits
    // just before its drop target -- moving down lands after the target
    // itself; moving up lands after the target's current predecessor.
    const afterId =
      activeIndex < overIndex ? items[overIndex].id : (items[overIndex - 1]?.id ?? null);
    onReorder(Number(active.id), afterId);
  }

  if (items.length === 0) {
    return (
      <p className="px-3 py-6 text-center text-sm text-neutral-400 dark:text-neutral-600">
        Nothing queued. Promote a capture from the stream, or type a prompt to add one directly.
      </p>
    );
  }

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={closestCenter}
      onDragEnd={handleDragEnd}
    >
      <SortableContext items={items.map((c) => c.id)} strategy={verticalListSortingStrategy}>
        <div className="flex flex-col gap-2">
          {items.map((capture) => (
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
