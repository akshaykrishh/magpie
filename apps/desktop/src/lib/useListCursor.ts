import { useState } from "react";

// Gmail/Superhuman-style keyboard cursor: arrow keys move a highlight that
// is entirely independent of any checkbox multi-select the list also
// maintains (see the design spec's "cursor position is not the selection").
// `items` must already be in the list's *visual* top-to-bottom order --
// callers whose rendering groups items (e.g. by section) must pass an array
// reflecting that grouped order, not whatever flat/unsorted array the data
// happens to arrive in, or ArrowDown/ArrowUp will visibly skip around.
// Matches the design spec's guard for Task 24's single-key shortcuts ("none
// of these fire while focus is inside any <input>, <textarea>, or
// contenteditable element") -- applied here too, one task earlier, because
// arrow keys need it just as much: without it, ArrowUp/ArrowDown pressed
// while editing a multi-line capture body (a <textarea> nested inside the
// cursor-scoped list container) would have their native caret-movement
// default action canceled by this ancestor handler, breaking in-place
// multi-line editing to move an invisible list cursor instead.
function isTextEntryTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return true;
  return target.isContentEditable;
}

export function useListCursor<T extends { id: number }>(items: T[]) {
  const [cursorId, setCursorId] = useState<number | null>(null);

  function onKeyDown(e: React.KeyboardEvent) {
    if (items.length === 0) return;
    if (isTextEntryTarget(e.target)) return;
    const index = items.findIndex((i) => i.id === cursorId);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursorId(items[Math.min(index + 1, items.length - 1)]?.id ?? items[0].id);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursorId(items[Math.max(index - 1, 0)]?.id ?? items[0].id);
    }
  }

  return { cursorId, setCursorId, onKeyDown };
}
