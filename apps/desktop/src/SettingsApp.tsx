import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { api } from "./lib/api";
import { type ThemePreference, readStoredPreference, setThemePreference } from "./lib/theme";

/// A modifier key must be present in a hotkey spec for it to be a plausible
/// global shortcut at all -- this is a client-side sanity check only
/// (Task 28 owns real validation/collision-checking against the OS and the
/// other hotkey), so it just blocks the obviously-wrong "no modifier"
/// case rather than trying to fully validate the spec here.
const MODIFIER_PATTERN = /⌘|Ctrl|⌥|⇧|Cmd|Command|Control|Alt|Option|Shift/i;

function hasModifier(value: string): boolean {
  return MODIFIER_PATTERN.test(value);
}

/// The Settings window: shows the two global hotkeys and lets the user
/// rebind either one via the `set_hotkey` command. A failed rebind (no
/// modifier, or the OS refusing registration because another app already
/// owns the combo) surfaces as a visible inline error rather than a false
/// success, and the old binding is left in effect -- `set_hotkey` itself
/// guarantees this by rolling back before returning the error.
function SettingsApp() {
  const [capture, setCapture] = useState("");
  const [screenshot, setScreenshot] = useState("");
  // The last values confirmed (by the backend) to actually be registered --
  // used to know which field(s) changed and need saving, and to resync the
  // inputs if a save only partially succeeds.
  const [savedCapture, setSavedCapture] = useState("");
  const [savedScreenshot, setSavedScreenshot] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [savedMessage, setSavedMessage] = useState<string | null>(null);
  // Seeded from the synchronous localStorage mirror (see theme-boot.ts) so
  // this control never flashes the wrong selection while get_setting's IPC
  // round-trip is in flight -- the mirror is at most one change behind the
  // SQLite row, and by the time it could be wrong the user hasn't opened
  // Settings yet to see it.
  const [themePref, setThemePref] = useState<ThemePreference>(() => readStoredPreference());

  const loadSettings = () =>
    api.getHotkeySettings().then((settings) => {
      setCapture(settings.capture);
      setScreenshot(settings.screenshot);
      setSavedCapture(settings.capture);
      setSavedScreenshot(settings.screenshot);
      setLoaded(true);
    });

  useEffect(() => {
    loadSettings().catch((e) => setError(String(e)));
  }, []);

  const captureValid = hasModifier(capture);
  const screenshotValid = hasModifier(screenshot);
  const hasChanges = capture !== savedCapture || screenshot !== savedScreenshot;
  const canSave = loaded && captureValid && screenshotValid && hasChanges && !saving;

  async function handleSave() {
    setSaving(true);
    setError(null);
    setSavedMessage(null);
    try {
      if (capture !== savedCapture) {
        await invoke("set_hotkey", { kind: "capture", combo: capture });
        setSavedCapture(capture);
      }
      if (screenshot !== savedScreenshot) {
        await invoke("set_hotkey", { kind: "screenshot", combo: screenshot });
        setSavedScreenshot(screenshot);
      }
      setSavedMessage("Saved.");
    } catch (e) {
      // A failed rebind means the OLD binding is still what's actually
      // registered (set_hotkey rolls back before erroring) -- re-fetch so
      // the inputs reflect reality rather than the rejected value,
      // especially important if the other field's save already succeeded
      // above before this one failed.
      setError(String(e));
      await loadSettings().catch(() => {});
    } finally {
      setSaving(false);
    }
  }

  async function handleThemeChange(pref: ThemePreference) {
    setThemePref(pref);
    await setThemePreference(pref);
  }

  return (
    <main className="flex h-screen flex-col gap-4 overflow-hidden bg-ground p-4">
      <h1 className="font-display text-lg font-bold text-fg">Settings</h1>

      {error && <p className="text-body-sm text-danger">{error}</p>}

      <fieldset className="flex flex-col gap-2 text-body-sm text-fg">
        <legend className="font-mono-label text-label-sm tracking-label uppercase text-fg-faint">
          Appearance
        </legend>
        <div className="flex gap-1 rounded-sm border border-hairline bg-surface p-1">
          {(["system", "light", "dark"] as const).map((pref) => (
            <button
              key={pref}
              type="button"
              onClick={() => void handleThemeChange(pref)}
              aria-pressed={themePref === pref}
              className={
                "flex-1 rounded-xs px-2 py-1 text-body-sm capitalize transition-colors " +
                (themePref === pref
                  ? "bg-accent text-fg-on-accent"
                  : "text-fg-muted hover:bg-surface-hover")
              }
            >
              {pref}
            </button>
          ))}
        </div>
      </fieldset>

      <label className="flex flex-col gap-1 text-body-sm text-fg">
        Capture hotkey
        <input
          className="rounded-xs border border-hairline bg-surface px-2 py-1 font-mono-label text-body-sm text-fg"
          value={capture}
          onChange={(e) => setCapture(e.target.value)}
          disabled={!loaded}
        />
        {!captureValid && loaded && (
          <span className="text-label text-danger">
            Must include at least one modifier (⌘/Ctrl/⌥/⇧).
          </span>
        )}
      </label>

      <label className="flex flex-col gap-1 text-body-sm text-fg">
        Screenshot hotkey
        <input
          className="rounded-xs border border-hairline bg-surface px-2 py-1 font-mono-label text-body-sm text-fg"
          value={screenshot}
          onChange={(e) => setScreenshot(e.target.value)}
          disabled={!loaded}
        />
        {!screenshotValid && loaded && (
          <span className="text-label text-danger">
            Must include at least one modifier (⌘/Ctrl/⌥/⇧).
          </span>
        )}
      </label>

      <div className="mt-auto flex items-center justify-end gap-3">
        {savedMessage && !error && (
          <span className="text-sm text-green-700 dark:text-green-400">{savedMessage}</span>
        )}
        <button
          type="button"
          disabled={!canSave}
          onClick={handleSave}
          className="rounded-xs bg-accent px-3 py-1.5 text-body-sm text-fg-on-accent disabled:cursor-not-allowed disabled:opacity-50"
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </main>
  );
}

export default SettingsApp;
