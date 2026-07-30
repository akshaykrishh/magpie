import { X, Zap } from "lucide-react";

interface PermissionBannerProps {
  onUpgrade: () => void;
  onDismiss: () => void;
}

export function PermissionBanner({ onUpgrade, onDismiss }: PermissionBannerProps) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-lg border border-amber-200 bg-amber-50 px-3 py-2 text-sm dark:border-amber-900 dark:bg-amber-950">
      <div className="flex items-center gap-2 text-amber-800 dark:text-amber-200">
        <Zap size={16} className="shrink-0" />
        <span>
          Capturing works, but needs copy-then-hotkey. Grant Accessibility for one-key capture.
        </span>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <button
          type="button"
          onClick={onUpgrade}
          className="rounded-md bg-amber-600 px-2.5 py-1 text-white hover:bg-amber-700"
        >
          Upgrade
        </button>
        <button
          type="button"
          onClick={onDismiss}
          className="rounded-md p-1 text-amber-700 hover:bg-amber-100 dark:text-amber-300 dark:hover:bg-amber-900"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
