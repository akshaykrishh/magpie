import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { api } from "./lib/api";

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

  return (
    <main className="flex h-screen flex-col gap-4 overflow-hidden bg-white p-4 dark:bg-neutral-950">
      <h1 className="font-display text-lg font-bold text-ink dark:text-white">Settings</h1>

      {error && <p className="text-sm text-red-600 dark:text-red-400">{error}</p>}

      <label className="flex flex-col gap-1 text-sm text-ink dark:text-neutral-200">
        Capture hotkey
        <input
          className="rounded border border-neutral-300 bg-white px-2 py-1 font-mono-label text-sm dark:border-neutral-700 dark:bg-neutral-900 dark:text-white"
          value={capture}
          onChange={(e) => setCapture(e.target.value)}
          disabled={!loaded}
        />
        {!captureValid && loaded && (
          <span className="text-xs text-red-600 dark:text-red-400">
            Must include at least one modifier (⌘/Ctrl/⌥/⇧).
          </span>
        )}
      </label>

      <label className="flex flex-col gap-1 text-sm text-ink dark:text-neutral-200">
        Screenshot hotkey
        <input
          className="rounded border border-neutral-300 bg-white px-2 py-1 font-mono-label text-sm dark:border-neutral-700 dark:bg-neutral-900 dark:text-white"
          value={screenshot}
          onChange={(e) => setScreenshot(e.target.value)}
          disabled={!loaded}
        />
        {!screenshotValid && loaded && (
          <span className="text-xs text-red-600 dark:text-red-400">
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
          className="rounded bg-slate-teal px-3 py-1.5 text-sm text-white disabled:cursor-not-allowed disabled:opacity-50 dark:bg-slate-teal-light"
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </main>
  );
}

export default SettingsApp;
