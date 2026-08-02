import { Kbd } from "./ui";
import { Logo, Wordmark } from "./Logo";

interface TitleBarProps {
  onOpenPalette: () => void;
}

/// `data-tauri-drag-region` makes the bar itself draggable (standard for a
/// custom/overlay title bar -- without it, only the traffic lights
/// themselves would move the window). The left padding clears macOS's
/// overlay-style traffic lights (tauri.conf.json's main window now sets
/// `titleBarStyle: "Overlay"` + `hiddenTitle: true`) -- they render over
/// the webview at a fixed position, so content needs to start clear of
/// them rather than the window reserving native space for a title bar.
export function TitleBar({ onOpenPalette }: TitleBarProps) {
  return (
    <header
      data-tauri-drag-region
      className="flex shrink-0 items-center gap-1.5 border-b border-hairline py-2 pl-20 pr-3"
    >
      <Logo size={18} />
      <Wordmark className="text-sm font-bold text-fg" />
      <div className="flex-1" />
      <button
        type="button"
        onClick={onOpenPalette}
        className="flex items-center gap-2 rounded-md border border-hairline px-2.5 py-1 text-xs text-fg-faint hover:border-accent-line hover:text-fg-muted"
      >
        Search, promote, run a template…
        <Kbd>⌘K</Kbd>
      </button>
    </header>
  );
}
