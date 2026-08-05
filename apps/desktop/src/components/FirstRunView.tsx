import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { Kbd, Mono } from "./ui";

interface FirstRunViewProps {
  onDone: () => void;
}

// `CommandOrControl+Shift+M` (the tauri-plugin-global-shortcut parse
// format the setting is actually stored in -- see lib.rs's HOTKEY consts)
// rendered as `⌘⇧M`, mirroring the Kbd primitive's own glyph-first style
// rather than showing a raw config string on someone's very first screen.
const MODIFIER_GLYPHS: Record<string, string> = {
  commandorcontrol: "⌘",
  command: "⌘",
  cmd: "⌘",
  control: "⌃",
  ctrl: "⌃",
  alt: "⌥",
  option: "⌥",
  shift: "⇧",
};

function formatHotkey(raw: string): string {
  return raw
    .split("+")
    .map((part) => MODIFIER_GLYPHS[part.toLowerCase()] ?? part.toUpperCase())
    .join("");
}

/// Shown once, in place of the main window's usual stream+Now layout,
/// until dismissed -- gated on the `onboarding_complete` setting (see
/// App.tsx), not on whether any captures exist yet, so it doesn't
/// reappear for someone who's captured plenty but genuinely emptied their
/// stream back out to zero.
export function FirstRunView({ onDone }: FirstRunViewProps) {
  const [captureHotkey, setCaptureHotkey] = useState("");
  const [screenshotHotkey, setScreenshotHotkey] = useState("");

  useEffect(() => {
    api
      .getHotkeySettings()
      .then((s) => {
        setCaptureHotkey(s.capture);
        setScreenshotHotkey(s.screenshot);
      })
      .catch(console.error);
  }, []);

  function handleDone() {
    api.setSetting("onboarding_complete", "true").catch(console.error);
    onDone();
  }

  return (
    <main className="flex h-screen flex-col items-center justify-center gap-8 px-10 text-center">
      <div className="flex flex-col gap-2">
        <Mono size="lg" tone="accent" weight="bold">
          MAGPIE
        </Mono>
        <h1 className="font-display text-title font-bold text-fg">
          Capture without breaking your flow
        </h1>
        <p className="max-w-md text-body text-fg-muted">
          Copy something, then press the hotkey — it lands in your stream, no window to switch to.
        </p>
      </div>

      <div className="flex flex-col gap-3">
        <div className="flex items-center justify-center gap-3">
          <Kbd>{captureHotkey ? formatHotkey(captureHotkey) : "…"}</Kbd>
          <span className="text-body-sm text-fg-muted">Capture whatever's on the clipboard</span>
        </div>
        <div className="flex items-center justify-center gap-3">
          <Kbd>{screenshotHotkey ? formatHotkey(screenshotHotkey) : "…"}</Kbd>
          <span className="text-body-sm text-fg-muted">Capture a screenshot region</span>
        </div>
      </div>

      <p className="max-w-md text-body-sm text-fg-faint">
        Nothing is ever filed automatically — magpie only ever proposes a destination, one tap
        confirms it.
      </p>

      <p className="max-w-md text-body-sm text-fg-faint">
        This window steps aside once you're back at work — the tray icon always brings it back,
        and the hotkeys above work from anywhere.
      </p>

      <button
        type="button"
        onClick={handleDone}
        className="rounded-md bg-accent px-4 py-2 text-body font-medium text-fg-on-accent hover:opacity-90"
      >
        Get started
      </button>
    </main>
  );
}
