import { useState } from "react";
import { Mono } from "@/components/ui";
import { api } from "@/lib/api";
import { parseDiffStat } from "@/lib/format";
import type { Capture } from "@/lib/types";
import { useOverlay } from "./useOverlay";

interface ReviewSheetProps {
  capture: Capture;
  /** Refetch whatever lists this capture appears in (stream/Now), same
      contract as every other mutation handler in this app. */
  onResolved: () => void;
}

/// The hand-back review sheet -- undesigned in the source mockup (its own
/// "try next" list says "show the hand-back review sheet"), specified here
/// from what's honestly available: the capture body (what was asked),
/// the agent's note (why it wants eyes), and `diff_stat` -- the full
/// `git diff --stat` text `capture_handback` stores, not just a summary
/// line. NOT available and not faked: the diff itself. magpie-core never
/// shells out to git and this app has no repo access -- that's a feature,
/// not a gap, since it keeps this sheet from pretending to be a code
/// review tool. The real review happens where review happens; this sheet's
/// job is "something is waiting, here's what changed, here's why," then
/// getting out of the way.
export function ReviewSheet({ capture, onResolved }: ReviewSheetProps) {
  const overlay = useOverlay();
  const [busy, setBusy] = useState(false);
  const diff = parseDiffStat(capture.diff_stat);

  async function accept() {
    setBusy(true);
    try {
      await api.markCaptureDone(capture.id);
      onResolved();
      overlay.pop();
    } finally {
      setBusy(false);
    }
  }

  async function sendBack() {
    setBusy(true);
    try {
      await api.sendBackForRework(capture.id);
      onResolved();
      overlay.pop();
    } finally {
      setBusy(false);
    }
  }

  function copyDiffCommand() {
    if (!capture.lease_head_commit) return;
    api.copyText(`git diff ${capture.lease_head_commit}`).catch(console.error);
  }

  return (
    <div className="flex max-h-[70vh] w-[560px] flex-col gap-4 overflow-y-auto p-4">
      <div>
        <Mono size="sm" tone="faint">
          CAPTURE
        </Mono>
        <p className="mt-1 text-body text-fg">{capture.body}</p>
      </div>

      {capture.handback_note && (
        <div className="border-l-2 border-accent-line pl-3">
          <Mono size="sm" tone="faint">
            WHY IT'S HERE
          </Mono>
          <p className="mt-1 text-body-sm text-fg-muted">{capture.handback_note}</p>
        </div>
      )}

      <div>
        <div className="flex items-center justify-between">
          <Mono size="sm" tone="faint">
            CHANGES
          </Mono>
          {diff && (diff.insertions > 0 || diff.deletions > 0) && (
            <Mono size="sm" tone="accent">
              +{diff.insertions} −{diff.deletions}
            </Mono>
          )}
        </div>
        {diff && diff.files.length > 0 ? (
          <div className="mt-2 flex flex-col gap-1">
            {diff.files.map((f) => (
              <div
                key={f.path}
                className="flex items-center justify-between rounded-xs bg-surface px-2 py-1"
              >
                <Mono size="sm" tone="fg" wide={false}>
                  {f.path}
                </Mono>
                <Mono size="xs" tone="faint" wide={false}>
                  {f.churn}
                </Mono>
              </div>
            ))}
          </div>
        ) : (
          <p className="mt-2 text-body-sm text-fg-faint">
            {capture.diff_stat ?? "No diff summary was recorded."}
          </p>
        )}
      </div>

      <div className="flex items-center justify-between border-t border-hairline pt-3">
        <button
          type="button"
          onClick={copyDiffCommand}
          disabled={!capture.lease_head_commit}
          className="text-body-sm text-fg-muted hover:text-fg disabled:cursor-not-allowed disabled:opacity-40"
        >
          Copy diff command
        </button>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={sendBack}
            disabled={busy}
            className="rounded-xs border border-hairline-strong px-3 py-1.5 text-body-sm text-fg disabled:opacity-50"
          >
            Send back
          </button>
          <button
            type="button"
            onClick={accept}
            disabled={busy}
            className="rounded-xs bg-accent px-3 py-1.5 text-body-sm text-fg-on-accent disabled:opacity-50"
          >
            Accept
          </button>
        </div>
      </div>
    </div>
  );
}
