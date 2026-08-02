import type { DragEndEvent } from "@dnd-kit/core";

// Groups items into per-section buckets (in the order the items themselves
// already arrive in, i.e. still `queue_pos` order -- grouping never
// introduces a second ordering dimension) plus a leftover bucket for items
// with no section. Was previously copy-pasted identically into App.tsx,
// NowList.tsx, and TemplatesPanel.tsx; "grouped by section, unsectioned
// items last, no dimension added to their existing order" means the same
// thing everywhere it's used, so it's one function.
//
// Takes an explicit `sectionIdOf` selector rather than requiring `T` to
// have a top-level `section_id` field -- StreamView groups `StreamRow[]`
// (see stream.ts), whose section id lives at `row.capture.section_id`,
// while NowList and TemplatesSheet still group plain `Capture[]`/
// `Template[]` with `section_id` at the top level. One function, two
// shapes, no duplicate grouping logic per shape.
export function groupBySection<T>(items: T[], sectionIdOf: (item: T) => number | null) {
  const bySection = new Map<number, T[]>();
  const unsectioned: T[] = [];
  for (const item of items) {
    const sectionId = sectionIdOf(item);
    if (sectionId === null) unsectioned.push(item);
    else bySection.set(sectionId, [...(bySection.get(sectionId) ?? []), item]);
  }
  return { bySection, unsectioned };
}

// Shared by every drag-to-reorder list (a section's own captures, the
// unsectioned leftovers, and the section headers themselves) -- computes
// "the id this element should now sit immediately after" from a plain
// DragEndEvent plus the *local* ordered array that list is rendering.
// `undefined` means "no-op" (dropped on itself, or off-target). Was
// previously duplicated in NowList.tsx and TemplatesPanel.tsx.
export function afterIdFromDragEnd<T extends { id: number }>(
  localOrder: T[],
  event: DragEndEvent,
): number | null | undefined {
  const { active, over } = event;
  if (!over || active.id === over.id) return undefined;

  const activeIndex = localOrder.findIndex((c) => c.id === active.id);
  const overIndex = localOrder.findIndex((c) => c.id === over.id);
  if (activeIndex === -1 || overIndex === -1) return undefined;

  return activeIndex < overIndex
    ? localOrder[overIndex].id
    : (localOrder[overIndex - 1]?.id ?? null);
}
