// Cross-window sync: the main window and the pinned dock each hold their
// own copy of Now in React state, so a mutation made in one (promote,
// reorder, done, demote, a typed prompt) has to tell the other to refetch.
// A plain Tauri event broadcasts to every window listening, which is all
// this needs -- no shared state store required for two windows.
export const NOW_CHANGED_EVENT = "now:changed";
