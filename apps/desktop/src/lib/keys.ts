// Shared guard: is this keyboard event's target somewhere text entry is
// actually happening? Used by every global/list-scoped single-key handler
// in the app (useListCursor's Space/Enter/single-key actions, the stream's
// type-ahead search, and the redesign's ⌘K/⌘L/⌥-prefixed global shortcuts)
// so a letter typed while editing a capture body, renaming a section, or
// composing in the stream's search box types the letter -- it never
// double-fires as a shortcut. Was previously a private, non-exported
// function inside useListCursor.ts; lifted here so the stream's type-ahead
// handler and the new global-key hook (lib/useGlobalKeys.ts) share the
// exact same check rather than each re-deriving it slightly differently.
export function isTextEntryTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return true;
  return target.isContentEditable;
}
