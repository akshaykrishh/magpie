// Must be the first import of every window entry point (main.tsx,
// dock-main.tsx, settings-main.tsx, and later aim/across's entries) --
// import side effects run in source order, and this needs to apply `.dark`
// before React ever renders a single node.
//
// Why synchronous localStorage and not the SQLite `settings` row: Tauri
// IPC is async, so an `invoke("get_setting", ...)` here would resolve
// after first paint, meaning every window open would flash Paper and then
// swap to Slate a moment later. localStorage is a same-process, synchronous
// mirror written by `setThemePreference` (src/lib/theme.ts) every time the
// SQLite row is written, so it's never more than one preference-change
// behind the durable source of truth, and it's available before paint.
//
// The mirror can only go stale in one way: a fresh install/window with no
// mirror yet defaults to "system", which is also the correct SQLite
// default -- so there's no incorrect-flash case to guard against, only a
// slightly-behind one after the very first change, which self-corrects the
// moment `initThemeSync` (called from each *-main.tsx after this import)
// reads the real row.
import { bootTheme } from "./theme";

bootTheme();
