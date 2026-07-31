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

/// The Settings window: currently a read-only-in-effect display of the two
/// global hotkeys (Save is disabled until Task 28 wires up the actual
/// rebind command) -- this task's job is just to get the window opening
/// and showing the live values pulled from `get_hotkey_settings`.
function SettingsApp() {
  const [capture, setCapture] = useState("");
  const [screenshot, setScreenshot] = useState("");
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .getHotkeySettings()
      .then((settings) => {
        setCapture(settings.capture);
        setScreenshot(settings.screenshot);
        setLoaded(true);
      })
      .catch((e) => setError(String(e)));
  }, []);

  const captureValid = hasModifier(capture);
  const screenshotValid = hasModifier(screenshot);
  const canSave = loaded && captureValid && screenshotValid;

  return (
    <main className="flex h-screen flex-col gap-4 overflow-hidden bg-white p-4 dark:bg-neutral-950">
      <h1 className="font-display text-lg font-bold text-ink dark:text-white">Settings</h1>

      {error && (
        <p className="text-sm text-red-600 dark:text-red-400">
          Failed to load hotkey settings: {error}
        </p>
      )}

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

      <div className="mt-auto flex justify-end">
        <button
          type="button"
          disabled={!canSave}
          title="Rebinding lands in a later release"
          className="rounded bg-slate-teal px-3 py-1.5 text-sm text-white disabled:cursor-not-allowed disabled:opacity-50 dark:bg-slate-teal-light"
        >
          Save
        </button>
      </div>
    </main>
  );
}

export default SettingsApp;
