import { X } from "lucide-react";
import { useState } from "react";
import type { Blob as CaptureBlob, Capture } from "@/lib/types";
import { EditableBody } from "./EditableBody";
import { MarkdownBody } from "./MarkdownBody";

interface ExpandedCaptureModalProps {
  capture: Capture;
  /** Screenshot captures' image + OCR text. These are prefetched by
      CaptureRow's own blob-loading effect (see its `mightHaveBlob` effect)
      for the row's normal, non-expanded rendering -- passing them down here
      reuses that single fetch instead of re-requesting the same blob a
      second time just because the row happens to be expanded. Plain text
      captures have neither, hence optional/undefined. */
  blob?: CaptureBlob | null;
  imageUrl?: string | null;
  timestamp: string;
  onClose: () => void;
  onSave: (body: string) => Promise<void>;
  /** Task 20's "Edit in New Window" reuses this same modal as a stand-in
      for a dedicated second window (no real second-window infra exists
      yet -- see CaptureRow's openEditWindow comment), opening it already
      in edit mode. Defaults to false for the plain "Expand" entry point,
      which should always start as a read-only preview. */
  startEditing?: boolean;
}

export function ExpandedCaptureModal({
  capture,
  blob,
  imageUrl,
  timestamp,
  onClose,
  onSave,
  startEditing = false,
}: ExpandedCaptureModalProps) {
  const [editing, setEditing] = useState(startEditing);

  // A screenshot's visible content is the image + OCR text (blob.ocr_text),
  // not capture.body (which stays "" for these -- see CaptureRow's
  // mightHaveBlob comment), so there's nothing meaningful to edit here.
  // Session digests are similarly not user-editable prose. Matches the
  // `editActionDisabled` gating CaptureRow already applies to the
  // Edit/Edit-in-New-Window context-menu entries for the same captures.
  const canEdit = !blob && capture.kind !== "session_digest";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
      onClick={onClose}
    >
      <div
        className="flex max-h-[80vh] w-full max-w-xl flex-col gap-3 overflow-y-auto rounded-lg border
                   border-neutral-200 bg-white p-4 shadow-xl dark:border-neutral-800 dark:bg-neutral-900"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-2">
          <p className="text-xs text-neutral-400 dark:text-neutral-500">{timestamp}</p>
          <button
            type="button"
            onClick={onClose}
            className="rounded p-1 text-neutral-400 hover:bg-neutral-100 dark:hover:bg-neutral-800"
          >
            <X size={16} />
          </button>
        </div>
        {imageUrl && (
          <img
            src={imageUrl}
            alt="Screenshot capture"
            className="max-h-96 w-fit max-w-full rounded-md border border-neutral-200 object-contain dark:border-neutral-800"
          />
        )}
        {blob ? (
          <MarkdownBody
            text={blob.ocr_text ?? "Reading text…"}
            className="text-sm text-neutral-500 dark:text-neutral-400"
          />
        ) : (
          <EditableBody
            capture={capture}
            editing={editing}
            onSave={async (body) => {
              await onSave(body);
              setEditing(false);
            }}
            onCancel={() => setEditing(false)}
          />
        )}
        {!editing && canEdit && (
          <button
            type="button"
            onClick={() => setEditing(true)}
            className="self-start text-xs text-slate-teal hover:underline dark:text-slate-teal-light"
          >
            Edit
          </button>
        )}
      </div>
    </div>
  );
}
