import { listen } from "@tauri-apps/api/event";
import { formatDistanceToNow } from "date-fns";
import { ArrowUpToLine, Check, GripVertical, X } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { CAPTURE_UPDATED_EVENT } from "@/lib/events";
import type { Blob as CaptureBlob, Capture } from "@/lib/types";
import { cn } from "@/lib/utils";

interface CaptureItemProps {
  capture: Capture;
  selected?: boolean;
  onToggleSelect?: (id: number) => void;
  onPromote?: (id: number) => void;
  onDone?: (id: number) => void;
  onDemote?: (id: number) => void;
  /** Drag handle + listeners, supplied by a sortable wrapper in the Now list. */
  dragHandleProps?: React.HTMLAttributes<HTMLButtonElement>;
  className?: string;
}

export function CaptureItem({
  capture,
  selected = false,
  onToggleSelect,
  onPromote,
  onDone,
  onDemote,
  dragHandleProps,
  className,
}: CaptureItemProps) {
  const timestamp = formatDistanceToNow(new Date(capture.created_at), {
    addSuffix: true,
  });

  const [blob, setBlob] = useState<CaptureBlob | null>(null);
  const [imageUrl, setImageUrl] = useState<string | null>(null);

  // A screenshot capture always has an empty body (see capture_flow.rs's
  // on_screenshot_hotkey) -- checking that first skips a blob lookup IPC
  // round trip for the overwhelming common case of a plain text capture.
  const mightHaveBlob = capture.body === "";

  useEffect(() => {
    if (!mightHaveBlob) return;
    let cancelled = false;

    function load() {
      api.getCaptureBlob(capture.id).then((b) => {
        if (!cancelled) setBlob(b);
      });
      api.getBlobImageDataUrl(capture.id).then((url) => {
        if (!cancelled) setImageUrl(url);
      });
    }
    load();

    // OCR finishes after the capture already exists -- this is what
    // updates the "reading text..." placeholder below once it lands.
    const unlisten = listen<number>(CAPTURE_UPDATED_EVENT, (event) => {
      if (event.payload === capture.id) load();
    });
    return () => {
      cancelled = true;
      unlisten.then((f) => f());
    };
  }, [capture.id, mightHaveBlob]);

  return (
    <div
      className={cn(
        "group flex items-start gap-2 rounded-lg border border-neutral-200 bg-white px-3 py-2.5",
        "dark:border-neutral-800 dark:bg-neutral-900",
        selected && "border-slate-teal ring-1 ring-slate-teal dark:border-slate-teal-light dark:ring-slate-teal-light",
        className,
      )}
    >
      {dragHandleProps && (
        <button
          type="button"
          className="mt-0.5 shrink-0 cursor-grab text-neutral-300 hover:text-neutral-500 dark:text-neutral-700 dark:hover:text-neutral-400"
          {...dragHandleProps}
        >
          <GripVertical size={16} />
        </button>
      )}

      {onToggleSelect && (
        <input
          type="checkbox"
          checked={selected}
          onChange={() => onToggleSelect(capture.id)}
          className="mt-1 size-4 shrink-0 accent-slate-teal"
        />
      )}

      <div className="min-w-0 flex-1">
        {blob ? (
          <div className="flex flex-col gap-1.5">
            {imageUrl && (
              <img
                src={imageUrl}
                alt="Screenshot capture"
                className="max-h-48 w-fit max-w-full rounded-md border border-neutral-200 object-contain dark:border-neutral-800"
              />
            )}
            <p className="whitespace-pre-wrap break-words text-xs leading-snug text-neutral-500 dark:text-neutral-400">
              {blob.ocr_text ?? "Reading text…"}
            </p>
          </div>
        ) : (
          <p className="whitespace-pre-wrap break-words text-sm leading-snug text-neutral-800 dark:text-neutral-200">
            {capture.body}
          </p>
        )}
        <p className="mt-1 text-xs text-neutral-400 dark:text-neutral-500">
          {timestamp}
        </p>
      </div>

      <div className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity group-hover:opacity-100">
        {onDone && (
          <button
            type="button"
            title="Mark done"
            onClick={() => onDone(capture.id)}
            className="rounded p-1.5 text-neutral-400 hover:bg-green-50 hover:text-green-600 dark:hover:bg-green-950"
          >
            <Check size={16} />
          </button>
        )}
        {onPromote && (
          <button
            type="button"
            title="Promote to Now"
            onClick={() => onPromote(capture.id)}
            className="rounded p-1.5 text-neutral-400 hover:bg-slate-teal/10 hover:text-slate-teal dark:hover:bg-slate-teal-light/15"
          >
            <ArrowUpToLine size={16} />
          </button>
        )}
        {onDemote && (
          <button
            type="button"
            title="Remove from Now"
            onClick={() => onDemote(capture.id)}
            className="rounded p-1.5 text-neutral-400 hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950"
          >
            <X size={16} />
          </button>
        )}
      </div>
    </div>
  );
}
