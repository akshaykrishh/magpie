import { listen } from "@tauri-apps/api/event";
import { formatDistanceToNow } from "date-fns";
import { GripVertical } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { CAPTURE_UPDATED_EVENT } from "@/lib/events";
import type { Blob as CaptureBlob, Capture } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ContextMenu, ContextMenuTrigger, type ContextMenuItem } from "./ContextMenu";
import { MarkdownBody } from "./MarkdownBody";

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

  const menuOpenRef = useRef<((x: number, y: number) => void) | null>(null);

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
      onContextMenu={(e) => {
        e.preventDefault();
        menuOpenRef.current?.(e.clientX, e.clientY);
      }}
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
            <MarkdownBody
              text={blob.ocr_text ?? "Reading text…"}
              className="text-xs text-neutral-500 dark:text-neutral-400"
            />
          </div>
        ) : (
          <MarkdownBody
            text={capture.body}
            className="text-sm text-neutral-800 dark:text-neutral-200"
          />
        )}
        <p className="mt-1 text-xs text-neutral-400 dark:text-neutral-500">
          {timestamp}
        </p>
      </div>

      <div className="shrink-0">
        <ContextMenuTrigger onOpen={(x, y) => menuOpenRef.current?.(x, y)} />
        <ContextMenu items={buildContextMenuItems()} openRef={menuOpenRef} />
      </div>
    </div>
  );

  function buildContextMenuItems(): ContextMenuItem[] {
    const items: ContextMenuItem[] = [];
    if (onDone) items.push({ label: "Mark Done", onClick: () => onDone(capture.id) });
    if (onPromote) items.push({ label: "Promote to Now", onClick: () => onPromote(capture.id) });
    if (onDemote) items.push({ label: "Remove from Now", onClick: () => onDemote(capture.id) });
    return items;
  }
}
