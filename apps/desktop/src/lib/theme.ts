import { api } from "./api";
import { THEME_CHANGED_EVENT } from "./events";

/// What the user asked for. "system" (the default) means "follow the OS",
/// not "dark until I say otherwise" -- the redesign is authored against
/// Slate (dark), but committing to dark-always would fight a user whose OS
/// is set to light, so System is the honest default and Dark/Light are
/// explicit overrides.
export type ThemePreference = "system" | "light" | "dark";

/// What actually gets applied to <html>. Distinct from ThemePreference
/// because "system" resolves to one of these two depending on the OS.
export type ThemeMode = "light" | "dark";

const STORAGE_KEY = "magpie.theme";
const MEDIA_QUERY = "(prefers-color-scheme: dark)";

function systemMode(): ThemeMode {
  return window.matchMedia(MEDIA_QUERY).matches ? "dark" : "light";
}

export function resolveMode(pref: ThemePreference): ThemeMode {
  return pref === "system" ? systemMode() : pref;
}

export function applyMode(mode: ThemeMode): void {
  document.documentElement.classList.toggle("dark", mode === "dark");
}

/// Reads the localStorage mirror synchronously -- see theme-boot.ts for why
/// this, not the SQLite `settings` row, is what boot uses.
export function readStoredPreference(): ThemePreference {
  const raw = localStorage.getItem(STORAGE_KEY);
  return raw === "light" || raw === "dark" || raw === "system" ? raw : "system";
}

// Live only while pref === "system": re-applies the mode whenever the OS
// appearance flips. Torn down and rebuilt on every call to
// `applyPreference` so exactly one watcher is ever active per window,
// regardless of how many times the preference changes underneath it.
let stopSystemWatch: (() => void) | null = null;

/// The single place that both applies a preference to this window's DOM
/// and manages the OS-change watcher's lifecycle. Every path that changes
/// or restores a preference (boot, reconciliation, the cross-window event
/// listener, and setThemePreference itself) goes through this rather than
/// calling applyMode directly, so the watcher can never be left running
/// for a preference that's no longer "system" (which would silently
/// override an explicit Light/Dark choice the next time the OS flips) or
/// leaked across repeated preference changes.
function applyPreference(pref: ThemePreference): void {
  stopSystemWatch?.();
  stopSystemWatch = null;
  applyMode(resolveMode(pref));
  if (pref === "system") {
    const mql = window.matchMedia(MEDIA_QUERY);
    const listener = () => applyMode(systemMode());
    mql.addEventListener("change", listener);
    stopSystemWatch = () => mql.removeEventListener("change", listener);
  }
}

/// Applies `pref` to this window immediately, persists it to both the
/// localStorage mirror (for the next window's synchronous boot) and the
/// SQLite `settings` row (the durable source of truth, and what a
/// freshly-installed window with no mirror yet falls back to), then tells
/// every other open window to do the same via a Tauri event -- localStorage
/// is per-window in a multi-webview Tauri app, it does not itself sync
/// across windows.
export async function setThemePreference(pref: ThemePreference): Promise<void> {
  localStorage.setItem(STORAGE_KEY, pref);
  applyPreference(pref);
  await api.setSetting("theme", pref);
  const { emit } = await import("@tauri-apps/api/event");
  await emit(THEME_CHANGED_EVENT, pref);
}

/// Called synchronously by theme-boot.ts, before first paint. Everything
/// theme-boot.ts needs is synchronous (localStorage, matchMedia), which is
/// exactly why it exists as a separate first-import module rather than
/// this one doing the work inline -- see theme-boot.ts's own comment.
export function bootTheme(): void {
  applyPreference(readStoredPreference());
}

/// Called once per window, after theme-boot.ts has already applied the
/// synchronous localStorage guess. Does two things theme-boot.ts can't,
/// because both need the async Tauri IPC bridge:
///
/// 1. Reconciles against the real SQLite `settings` row -- the only case
///    this actually changes anything is a fresh machine/profile with no
///    localStorage mirror yet, where boot's "system" default happens to
///    disagree with a preference set previously on a synced/restored copy
///    of the database.
/// 2. Subscribes to THEME_CHANGED_EVENT so a preference change made in one
///    window (e.g. Settings) is applied here too, since localStorage does
///    not itself sync across a multi-webview Tauri app's windows.
///
/// Returns an unsubscribe function; call it from a `useEffect` cleanup (or
/// just let the window close) rather than leaking the listener.
export async function initThemeSync(): Promise<() => void> {
  const stored = await api.getSetting("theme");
  if (
    (stored === "light" || stored === "dark" || stored === "system") &&
    stored !== readStoredPreference()
  ) {
    localStorage.setItem(STORAGE_KEY, stored);
    applyPreference(stored);
  }

  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<ThemePreference>(THEME_CHANGED_EVENT, (event) => {
    localStorage.setItem(STORAGE_KEY, event.payload);
    applyPreference(event.payload);
  });
  return unlisten;
}
