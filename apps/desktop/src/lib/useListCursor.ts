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

// Task 24's Space/Enter/single-key shortcuts -- all optional so existing
// callers (NowList.tsx) that just want arrow-key cursor movement can keep
// calling useListCursor(items) with no second argument at all.
export interface ListCursorActions {
  /** Space: toggle the cursor row's checkbox, without moving the cursor. */
  onToggleSelect?: (id: number) => void;
  /** Enter: expand the cursor row (same as its own "Expand" menu item). */
  onExpand?: (id: number) => void;
  /** Any other single key (e.g. "d", "c", "C", "e", "M", "Backspace",
      "Delete") -- callers switch on `key` themselves, mirroring whichever
      subset of the context-menu action set they want reachable this way.
      Never fired for Space/Enter/ArrowUp/ArrowDown, which are handled above
      this branch. */
  onAction?: (key: string, id: number) => void;
}

export function useListCursor<T extends { id: number }>(
  items: T[],
  actions: ListCursorActions = {},
) {
  const [cursorId, setCursorId] = useState<number | null>(null);

  function onKeyDown(e: React.KeyboardEvent) {
    if (items.length === 0) return;
    // Same guard as arrows above: none of Space/Enter/single-key actions may
    // fire while focus is inside an <input>/<textarea>/contenteditable --
    // typing "d" while composing a capture body must type the letter "d",
    // not fire "mark done" on whatever row the invisible list cursor
    // happens to be sitting on.
    if (isTextEntryTarget(e.target)) return;
    const index = items.findIndex((i) => i.id === cursorId);
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursorId(items[Math.min(index + 1, items.length - 1)]?.id ?? items[0].id);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursorId(items[Math.max(index - 1, 0)]?.id ?? items[0].id);
    } else if (e.key === " ") {
      // Always preventDefault, even with no cursor row yet -- a focused,
      // non-form element's native Space action is to scroll the page, and
      // that default must not leak through just because ArrowUp/Down was
      // never pressed to establish a cursor row first.
      e.preventDefault();
      if (cursorId !== null) actions.onToggleSelect?.(cursorId);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (cursorId !== null) actions.onExpand?.(cursorId);
    } else if (cursorId !== null && !e.metaKey && !e.ctrlKey && !e.altKey) {
      // Modifier-held combinations (Cmd/Ctrl/Alt+<letter>) are left alone --
      // e.g. Cmd+C must stay the browser/OS copy shortcut, not this list's
      // single-key "c" action, even though e.key reports the same letter.
      actions.onAction?.(e.key, cursorId);
    }
  }

  return { cursorId, setCursorId, onKeyDown };
}
