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
  // NOTE: distinguishing "a new toast" from "the same toast, parent
  // re-rendered" must NOT be done via this dependency array (e.g. adding
  // `message`) -- content can't tell those apart when two different
  // showings pass equal values (in practice every caller currently passes
  // the same literal message text), and doing it that way silently drops
  // the countdown reset for same-text back-to-back toasts. The caller
  // (App.tsx) is responsible for giving each logically distinct toast a
  // different `key`, which forces React to unmount/remount this component
  // -- a fresh mount always re-runs this effect from scratch, regardless of
  // whether onDismiss/durationMs happen to compare equal to the last
  // showing's.
  useEffect(() => {
    const handle = setTimeout(onDismiss, durationMs);
    return () => clearTimeout(handle);
  }, [onDismiss, durationMs]);

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
