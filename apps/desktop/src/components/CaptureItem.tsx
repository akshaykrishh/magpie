import { listen } from "@tauri-apps/api/event";
import { formatDistanceToNow } from "date-fns";
import { GripVertical, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { CAPTURE_UPDATED_EVENT } from "@/lib/events";
import type { Blob as CaptureBlob, Capture, Project, Section } from "@/lib/types";
import { cn } from "@/lib/utils";
import { ContextMenu, ContextMenuTrigger, type ContextMenuItem } from "./ContextMenu";
import { EditableBody } from "./EditableBody";
import { MarkdownBody } from "./MarkdownBody";

interface CaptureItemProps {
  capture: Capture;
  selected?: boolean;
  onToggleSelect?: (id: number) => void;
  /** Every currently-checked id in the same list this row belongs to (not
      just whether *this* row is checked) -- lets the batch-aware context-menu
      actions (Copy as List, Merge Notes, Move to Project/Section, Delete) act
      on "the checked selection if non-empty, else just this row." Omitted
      entirely (e.g. the Now list, which has no selection UI at all) simply
      means every batch action falls back to acting on this row alone. */
  selectedIds?: Set<number>;
  onPromote?: (id: number) => void;
  onDone?: (id: number) => void;
  onDemote?: (id: number) => void;
  onReopen?: (id: number) => void;
  /** Persists an edited body (Task 16's `updateCaptureBody`) and refreshes
      whatever lists this capture may appear in -- owned by App.tsx, same
      shape as onDone/onPromote/etc., since saving an edit needs the same
      "mutate then refetch" the rest of this list already does. */
  onEdit?: (id: number, body: string) => Promise<void>;
  onMerge?: (ids: number[]) => void;
  onDelete?: (ids: number[]) => void;
  onMoveProject?: (ids: number[], projectId: number | null) => void;
  onMoveSection?: (ids: number[], sectionId: number | null) => void;
  onCreateSection?: (ids: number[], name: string) => void;
  projects?: Project[];
  sections?: Section[];
  /** Drag handle + listeners, supplied by a sortable wrapper in the Now list. */
  dragHandleProps?: React.HTMLAttributes<HTMLButtonElement>;
  className?: string;
}

export function CaptureItem({
  capture,
  selected = false,
  onToggleSelect,
  selectedIds,
  onPromote,
  onDone,
  onDemote,
  onReopen,
  onEdit,
  onMerge,
  onDelete,
  onMoveProject,
  onMoveSection,
  onCreateSection,
  projects = [],
  sections = [],
  dragHandleProps,
  className,
}: CaptureItemProps) {
  const timestamp = formatDistanceToNow(new Date(capture.created_at), {
    addSuffix: true,
  });

  const menuOpenRef = useRef<((x: number, y: number) => void) | null>(null);

  const [blob, setBlob] = useState<CaptureBlob | null>(null);
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [editing, setEditing] = useState(false);
  const [expanded, setExpanded] = useState(false);

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

  async function handleSaveEdit(body: string) {
    if (onEdit) await onEdit(capture.id, body);
    setEditing(false);
  }

  // Real "open a second OS window" infrastructure (a dedicated Tauri
  // WebviewWindow + route + a way to hydrate it with this one capture) isn't
  // wired up anywhere in this app yet -- every window today (main/toast/dock)
  // is declared statically in tauri.conf.json and vite.config.ts's
  // rollupOptions.input, and there's no "get a single capture by id" command
  // to hand that new window its data. Building all of that is out of scope
  // for wiring an existing action into a menu, so this opens the same Expand
  // modal already in edit mode as a stand-in -- a real new-window
  // implementation is a follow-up, not something this task should invent.
  function openEditWindow() {
    setExpanded(true);
    setEditing(true);
  }

  function createAndAssignSection() {
    const name = window.prompt("New section name");
    if (name?.trim()) onCreateSection?.(targetIds(), name.trim());
  }

  // "Act on the checked selection if non-empty, else just this row" -- the
  // same rule every batch-aware action below applies consistently, so a
  // right-click on an unchecked row while other rows are checked acts on the
  // checked set (matching how the standalone MergeToolbar batch bar already
  // treats "checked" as the unit of action), while a plain right-click with
  // nothing checked acts on just the row you clicked.
  function targetIds(): number[] {
    return selectedIds && selectedIds.size > 0 ? Array.from(selectedIds) : [capture.id];
  }

  const isScreenshot = capture.body === "";
  const isSessionDigest = capture.kind === "session_digest";
  // Editing a screenshot's body doesn't mean anything -- its visible content
  // is the image + OCR text (blob.ocr_text), not capture.body, which stays
  // "" for these (see mightHaveBlob's comment above).
  const editDisabled = isSessionDigest || isScreenshot || !onEdit;

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
          // The row's own inline editor is suppressed while the Expand modal
          // is open (`!expanded`) so "Edit in New Window" doesn't leave two
          // independent draft textareas live at once -- see openEditWindow.
          <EditableBody
            capture={capture}
            editing={editing && !expanded}
            onSave={handleSaveEdit}
            onCancel={() => setEditing(false)}
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

      {expanded && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6"
          onClick={() => setExpanded(false)}
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
                onClick={() => setExpanded(false)}
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
                onSave={handleSaveEdit}
                onCancel={() => setEditing(false)}
              />
            )}
          </div>
        </div>
      )}
    </div>
  );

  function buildContextMenuItems(): ContextMenuItem[] {
    const ids = targetIds();
    const batchSuffix = ids.length > 1 ? ` (${ids.length})` : "";
    const items: ContextMenuItem[] = [];

    if (capture.done_at) {
      if (onReopen) items.push({ label: "Reopen", onClick: () => onReopen(capture.id) });
    } else if (onDone) {
      items.push({ label: "Mark Done", onClick: () => onDone(capture.id) });
    }

    if (capture.queue_pos === null) {
      if (onPromote) items.push({ label: "Promote to Now", onClick: () => onPromote(capture.id) });
    } else if (onDemote) {
      items.push({ label: "Remove from Now", onClick: () => onDemote(capture.id) });
    }

    items.push(
      {
        label: "Copy",
        onClick: () => (isScreenshot ? api.copyCaptureImage(capture.id) : api.copyCaptureText(capture.id)),
      },
      { label: `Copy as List${batchSuffix}`, onClick: () => api.copyCapturesAsChecklist(ids) },
      { label: "Edit", onClick: () => setEditing(true), disabled: editDisabled },
      { label: "Edit in New Window", onClick: () => openEditWindow(), disabled: editDisabled },
      { label: "Expand", onClick: () => setExpanded(true) },
      {
        label: `Merge Notes${batchSuffix}`,
        onClick: () => onMerge?.(ids),
        // merge_captures errors below 2 sources (crates/magpie-core/src/merge.rs)
        // -- a lone target id is never a valid merge, batch or not.
        disabled: !onMerge || ids.length < 2,
      },
      {
        label: "Move to Project",
        submenu: [
          { label: "Inbox", onClick: () => onMoveProject?.(ids, null), disabled: !onMoveProject },
          ...projects.map((p) => ({
            label: p.name,
            onClick: () => onMoveProject?.(ids, p.id),
            disabled: !onMoveProject,
          })),
        ],
      },
      {
        label: "Move to Section",
        submenu: [
          { label: "None", onClick: () => onMoveSection?.(ids, null), disabled: !onMoveSection },
          ...sections.map((s) => ({
            label: s.name,
            onClick: () => onMoveSection?.(ids, s.id),
            disabled: !onMoveSection,
          })),
          { label: "New section…", onClick: () => createAndAssignSection(), disabled: !onCreateSection },
        ],
      },
      {
        label: `Delete${batchSuffix}`,
        onClick: () => onDelete?.(ids),
        disabled: !onDelete,
        destructive: true,
      },
    );

    return items;
  }
}
