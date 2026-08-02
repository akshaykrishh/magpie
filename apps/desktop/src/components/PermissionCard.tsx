import { Mono } from "./ui";

interface PermissionCardProps {
  onUpgrade: () => void;
  onDismiss: () => void;
}

/// The permission upgrade offer, as the first slot in the stream rather
/// than a banner above it (see the redesign plan's stage 11) -- earned at
/// 24 clipboard-only captures (see `usePermissionOffer`), not requested on
/// a fixed capture count regardless of mode. Dismissing this card is
/// permanent; the offer itself isn't lost, it moves into ⌘K (see
/// CommandPalette.tsx's own `showInPalette`-gated entry).
export function PermissionCard({ onUpgrade, onDismiss }: PermissionCardProps) {
  return (
    <div className="mb-2 flex items-center gap-3 rounded-md border border-accent-line bg-accent-tint px-3 py-2.5">
      <Mono size="sm" tone="accent" weight="bold">
        ONE-KEY CAPTURE
      </Mono>
      <span className="flex-1 text-body-sm text-fg-muted">
        Capturing works, but needs copy-then-hotkey. Grant Accessibility to capture with one key.
      </span>
      <button
        type="button"
        onClick={onUpgrade}
        className="rounded-sm bg-accent px-2.5 py-1 text-body-sm text-fg-on-accent hover:opacity-90"
      >
        Upgrade
      </button>
      <button
        type="button"
        onClick={onDismiss}
        className="rounded-sm px-2 py-1 text-body-sm text-fg-faint hover:bg-surface-hover"
      >
        Dismiss
      </button>
    </div>
  );
}
