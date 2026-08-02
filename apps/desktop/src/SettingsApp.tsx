import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import { api } from "./lib/api";
import { type ThemePreference, readStoredPreference, setThemePreference } from "./lib/theme";
import type { UpdateStatus } from "./lib/types";

/// A modifier key must be present in a hotkey spec for it to be a plausible
/// global shortcut at all -- this is a client-side sanity check only
/// (Task 28 owns real validation/collision-checking against the OS and the
/// other hotkey), so it just blocks the obviously-wrong "no modifier"
/// case rather than trying to fully validate the spec here.
const MODIFIER_PATTERN = /⌘|Ctrl|⌥|⇧|Cmd|Command|Control|Alt|Option|Shift/i;

function hasModifier(value: string): boolean {
  return MODIFIER_PATTERN.test(value);
}

/// One line describing the current update state -- kept as a plain
/// function (not JSX) since it's used both for the visible status text
/// and could be reused for a title/tooltip without re-deriving the switch.
function describeUpdateStatus(status: UpdateStatus): string {
  switch (status.kind) {
    case "idle":
      return "Not checked yet.";
    case "checking":
      return "Checking for updates…";
    case "up_to_date":
      return "Up to date.";
    case "available":
      return `Downloading ${status.version}…`;
    case "downloading": {
      if (status.total && status.total > 0) {
        const pct = Math.round((status.downloaded / status.total) * 100);
        return `Downloading update… ${pct}%`;
      }
      return "Downloading update…";
    }
    case "ready":
      return `${status.version} is ready to install.`;
    case "skipped":
      return `${status.version} is available (skipped).`;
    case "unsupported":
      return status.reason;
    case "failed":
      return `Update check failed: ${status.message}`;
  }
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
  const [appVersion, setAppVersion] = useState("");
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>({ kind: "idle" });
  const [autoCheck, setAutoCheck] = useState(true);
  const [updateBusy, setUpdateBusy] = useState(false);

  useEffect(() => {
    void getVersion().then(setAppVersion);
  }, []);

  useEffect(() => {
    void api.getUpdateStatus().then(setUpdateStatus);
    void api.getSetting("update_auto_check").then((v) => setAutoCheck(v !== "off"));

    const unlistenUpdate = listen<UpdateStatus>("update:status", (event) => {
      setUpdateStatus(event.payload);
    });
    return () => {
      unlistenUpdate.then((f) => f());
    };
  }, []);

  async function handleAutoCheckChange(checked: boolean) {
    setAutoCheck(checked);
    await api.setSetting("update_auto_check", checked ? "on" : "off");
  }

  async function handleCheckForUpdates() {
    setUpdateBusy(true);
    try {
      await api.checkForUpdates();
    } finally {
      setUpdateBusy(false);
    }
  }

  async function handleInstallUpdate() {
    setUpdateBusy(true);
    try {
      await api.installUpdate();
      // No `finally` resetting updateBusy on the success path -- a
      // successful install calls app.restart() on the Rust side, so this
      // window is about to be torn down anyway.
    } catch (e) {
      setUpdateBusy(false);
      setError(String(e));
    }
  }

  async function handleSkipUpdate(version: string) {
    await api.skipUpdateVersion(version);
  }

  async function handleUnskipUpdate() {
    await api.unskipUpdateVersion();
  }

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
    <main className="flex h-screen flex-col gap-4 overflow-y-auto bg-ground p-4">
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

      <fieldset className="flex flex-col gap-2 text-body-sm text-fg">
        <legend className="font-mono-label text-label-sm tracking-label uppercase text-fg-faint">
          About
        </legend>
        <div className="flex items-center justify-between">
          <span>Version {appVersion}</span>
          {appVersion && (
            <button
              type="button"
              onClick={() =>
                void openUrl(
                  `https://github.com/akshaykrishh/magpie/releases/tag/v${appVersion}`,
                )
              }
              className="text-fg-muted underline hover:text-fg"
            >
              Release notes
            </button>
          )}
        </div>
        {updateStatus.kind !== "unsupported" && (
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={autoCheck}
              onChange={(e) => void handleAutoCheckChange(e.target.checked)}
            />
            Automatically check for updates
          </label>
        )}
        <div className="flex items-center justify-between gap-2">
          <span className="text-fg-muted">{describeUpdateStatus(updateStatus)}</span>
          {updateStatus.kind === "ready" && (
            <button
              type="button"
              disabled={updateBusy}
              onClick={() => void handleInstallUpdate()}
              className="rounded-xs bg-accent px-3 py-1.5 text-body-sm text-fg-on-accent disabled:cursor-not-allowed disabled:opacity-50"
            >
              Install and relaunch
            </button>
          )}
          {updateStatus.kind === "available" && (
            <button
              type="button"
              onClick={() => void handleSkipUpdate(updateStatus.version)}
              className="text-fg-muted underline hover:text-fg"
            >
              Skip this version
            </button>
          )}
          {updateStatus.kind === "skipped" && (
            <button
              type="button"
              onClick={() => void handleUnskipUpdate()}
              className="text-fg-muted underline hover:text-fg"
            >
              Show update
            </button>
          )}
          {(updateStatus.kind === "idle" ||
            updateStatus.kind === "up_to_date" ||
            updateStatus.kind === "failed") && (
            <button
              type="button"
              disabled={updateBusy}
              onClick={() => void handleCheckForUpdates()}
              className="text-fg-muted underline hover:text-fg disabled:cursor-not-allowed disabled:opacity-50"
            >
              Check for updates
            </button>
          )}
        </div>
      </fieldset>

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
