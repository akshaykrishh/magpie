import { useEffect } from "react";

interface UndoToastProps {
  message: string;
  onUndo: () => void;
  onDismiss: () => void;
  /** How long before this toast auto-dismisses if Undo isn't clicked. */
  durationMs?: number;
}

export function UndoToast({
  message,
  onUndo,
  onDismiss,
  durationMs = 6000,
}: UndoToastProps) {
  useEffect(() => {
    const handle = setTimeout(onDismiss, durationMs);
    return () => clearTimeout(handle);
    // `message` is intentionally included even though it's unused in the
    // effect body: it's what distinguishes "the same toast is still up" from
    // "a new toast replaced it" when the caller doesn't unmount/remount this
    // component between toasts (see App.tsx, which keeps a single slot for
    // at most one toast). A new message should get its own full countdown;
    // an unrelated parent re-render that leaves message/onDismiss/durationMs
    // untouched should not reset the timer already in flight.
  }, [message, onDismiss, durationMs]);

  return (
    <div
      className="fixed bottom-4 left-1/2 z-50 flex -translate-x-1/2 items-center gap-3
                 rounded-lg bg-neutral-900 px-4 py-2.5 text-sm text-white shadow-lg
                 dark:bg-neutral-100 dark:text-neutral-900"
    >
      <span>{message}</span>
      <button
        type="button"
        onClick={onUndo}
        className="font-medium text-slate-teal-light underline hover:opacity-80 dark:text-slate-teal"
      >
        Undo
      </button>
    </div>
  );
}
