import { listen } from "@tauri-apps/api/event";
import { formatDistanceToNow } from "date-fns";
import { GripVertical } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
import { CAPTURE_UPDATED_EVENT } from "@/lib/events";
import type { NowRowState } from "@/lib/format";
import type { Blob as CaptureBlob, Capture, Project, Section, StreamRow } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Chip, Mono, StatusGlyph } from "./ui";
import { ContextMenu, ContextMenuTrigger, type ContextMenuItem } from "./ContextMenu";
import { EditableBody } from "./EditableBody";
import { ExpandedCaptureModal } from "./ExpandedCaptureModal";
import { MarkdownBody } from "./MarkdownBody";

interface CaptureRowProps {
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
  /** Persists an edited body and refreshes whatever lists this capture may
      appear in -- owned by the shell, same shape as onDone/onPromote/etc. */
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
  /** This row's `expanded` state has no external setter (its own "Expand"
      menu item just calls `setExpanded(true)` directly) -- bumping this to
      any new value (it need not be sequential, just different from the
      previous render) opens the same Expand modal, from outside the
      component, exactly as if that menu item had been clicked. `undefined`
      (the default) never triggers it, including on mount. */
  expandSignal?: number;
  /** Same mechanism as `expandSignal`, but for the row's own "Edit" item
      (`setEditing(true)`, the inline editor -- no modal). Ignored for
      screenshots/session digests, the same captures the "Edit" menu item
      itself disables for. */
  editSignal?: number;
  /** Whether this row is the list's current keyboard cursor (`useListCursor`'s
      `cursorId === capture.id`) -- entirely separate from `selected`
      (checkbox multi-select). Both can be true on the same row at once, so
      the two styles must be able to render simultaneously without one
      masking the other (see the outline-vs-ring choice below). */
  isCursor?: boolean;
  /** Opens this exact row's context menu from outside the component the
      same way expandSignal/editSignal reach in for Expand/Edit -- but
      positioning the menu needs an (x, y) computed from this row's
      on-screen location, not just a "fire" token, so the mechanism here is
      a registration callback: on mount, this row hands the caller a small
      function forwarding straight through to menuOpenRef, the identical
      imperative open the "..." button and right-click already use, keyed
      by capture.id. */
  onRegisterMenuOpen?: (id: number, open: ((x: number, y: number) => void) | null) => void;

  /// Present only when this row renders inside the Now column -- selects
  /// the five-state status glyph + state-specific chip row. See
  /// lib/format.ts's `nowRowState` for the derivation and priority order;
  /// computed by the caller (NowList), not re-derived here, since NowList
  /// is also where the session lookup for `leaseLabel` lives.
  nowState?: NowRowState;
  /// "LEASED BY S1 · 12M" -- only rendered when `nowState === "leased"`.
  /// Elapsed, never a countdown -- see lib/format.ts's `elapsed` doc
  /// comment on why leases have no expiry to count down to.
  leaseLabel?: string;
  /// The Now column's `⌥⌫ REVOKE` action -- only shown when `nowState ===
  /// "leased"`.
  onRevokeLease?: () => void;
  /// The hand-back review sheet's entry point -- only shown when
  /// `nowState === "handback"`.
  onOpenReview?: () => void;

  /// Present only in the stream -- the joined provenance stream.rs's
  /// `list_stream_rows` computes (source app, project name + filing
  /// confidence, session label, merge count, OCR-pending state). When
  /// given, its `blob_mime`/`ocr_text`/`blob_width`/`blob_height` are used
  /// instead of a per-row `get_capture_blob` IPC call -- part of the N+1
  /// fix `list_stream_rows` exists for. `get_blob_image_data_url` (the
  /// actual pixel data) is still fetched per-row below; gating that to
  /// on-screen rows via an IntersectionObserver is a real follow-up, not
  /// done in this pass -- see the redesign plan's stage 4 risk notes.
  streamRow?: StreamRow;
}

export function CaptureRow({
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
  expandSignal,
  editSignal,
  isCursor = false,
  onRegisterMenuOpen,
  nowState,
  leaseLabel,
  onRevokeLease,
  onOpenReview,
  streamRow,
}: CaptureRowProps) {
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
      // streamRow already carries blob_mime/ocr_text (see this component's
      // doc comment) -- skip the metadata round trip when it's available,
      // still fetch it directly for Now-column rows (no streamRow there).
      if (!streamRow) {
        api.getCaptureBlob(capture.id).then((b) => {
          if (!cancelled) setBlob(b);
        });
      }
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
    // eslint-disable-next-line react-hooks/exhaustive-deps -- streamRow's
    // identity churns every refetch; only its *presence* (fetch strategy)
    // should re-arm this effect, not its content.
  }, [capture.id, mightHaveBlob, !!streamRow]);

  async function handleSaveEdit(body: string) {
    if (onEdit) await onEdit(capture.id, body);
    setEditing(false);
  }

  // Real "open a second OS window" infrastructure isn't wired up anywhere
  // in this app yet -- every window is declared statically in
  // tauri.conf.json and vite.config.ts's rollupOptions.input, and there's
  // no "get a single capture by id" command to hand a new window its data.
  // This opens the same Expand modal already in edit mode as a stand-in.
  // Folding ExpandedCaptureModal into overlays/OverlayHost.tsx (per the
  // redesign plan's stage 5b notes) is deferred to whenever this stand-in
  // is finally replaced with a real window -- doing it now would mean
  // duplicating this row's own blob-fetching state at the host level for
  // no functional gain yet.
  function openEditWindow() {
    setExpanded(true);
    setEditing(true);
  }

  // Closing the modal must undo *both* halves of openEditWindow's stand-in,
  // not just `expanded` -- the row's own inline editor is gated on
  // `editing && !expanded` (see the render below), so leaving `editing` true
  // after the modal closes would flip that expression straight back to
  // true and drop the plain row into an inline-edit textarea the user never
  // asked for by closing what looks like a preview panel.
  function closeExpanded() {
    setExpanded(false);
    setEditing(false);
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
  // "" for these (see mightHaveBlob's comment above). Whether Edit appears
  // in the menu AT ALL is a separate question (gated on `onEdit` being
  // present at all, in buildContextMenuItems below) from whether it's
  // *disabled* for this particular capture's kind/shape.
  const editActionDisabled = isSessionDigest || isScreenshot;

  // Latest-value ref for the editSignal effect below -- deliberately NOT a
  // dependency of that effect. See the original design rationale: putting
  // either in the effect's deps would re-run it (and re-fire
  // setEditing(true)) on any unrelated re-render.
  const editGateRef = useRef({ onEdit, editActionDisabled });
  editGateRef.current = { onEdit, editActionDisabled };

  useEffect(() => {
    if (expandSignal !== undefined) setExpanded(true);
  }, [expandSignal]);

  useEffect(() => {
    const { onEdit: currentOnEdit, editActionDisabled: currentlyDisabled } = editGateRef.current;
    if (editSignal !== undefined && !currentlyDisabled && currentOnEdit) setEditing(true);
  }, [editSignal]);

  useEffect(() => {
    if (!onRegisterMenuOpen) return;
    onRegisterMenuOpen(capture.id, (x, y) => menuOpenRef.current?.(x, y));
    return () => onRegisterMenuOpen(capture.id, null);
  }, [capture.id, onRegisterMenuOpen]);

  // OCR-pending is only knowable from streamRow (joined) or blob (Now-row
  // fetch) -- `capture.body === ""` alone can't distinguish "still reading
  // text" from "no text was ever recognized."
  const ocrText = streamRow ? streamRow.ocr_text : blob?.ocr_text;
  const hasBlob = streamRow ? streamRow.blob_mime !== null : blob !== null;

  return (
    <div
      // Only given an id when this row is wired to the keyboard-cursor
      // mechanism (onRegisterMenuOpen present) -- a promoted capture can be
      // rendered twice on screen at once (once here, once as its own
      // separate CaptureRow in NowList's sidebar), and NowList has no
      // keyboard cursor of its own to need this id for.
      id={onRegisterMenuOpen ? `capture-${capture.id}` : undefined}
      onContextMenu={(e) => {
        e.preventDefault();
        menuOpenRef.current?.(e.clientX, e.clientY);
      }}
      className={cn(
        "group flex items-start gap-2 rounded-lg border border-neutral-200 bg-white px-3 py-2.5",
        "dark:border-neutral-800 dark:bg-neutral-900",
        nowState === "leased" && "bg-lease border-accent-line",
        nowState === "handback" && "border-accent-line bg-accent-tint",
        nowState === "pinned" && "opacity-50",
        selected &&
          "border-slate-teal ring-1 ring-slate-teal dark:border-slate-teal-light dark:ring-slate-teal-light",
        // Deliberately a native `outline` (a separate box property from the
        // `border`/`ring` `selected` uses above) in a neutral gray rather
        // than the brand teal -- so a row that's both the keyboard cursor
        // AND checked renders both cues at once instead of one clobbering
        // the other, and a sighted user can tell "cursor" (gray outline)
        // apart from "checked" (teal border+ring) at a glance rather than
        // seeing one merged/ambiguous highlight.
        isCursor && "outline outline-2 outline-offset-1 outline-neutral-400 dark:outline-neutral-500",
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

      {nowState && (
        <div className="mt-1 shrink-0">
          <StatusGlyph state={nowState} />
        </div>
      )}

      {onToggleSelect && (
        <input
          type="checkbox"
          checked={selected}
          onChange={() => onToggleSelect(capture.id)}
          className="mt-1 size-4 shrink-0 accent-accent"
        />
      )}

      <div className="min-w-0 flex-1">
        {isSessionDigest ? (
          <div className="rounded-md border border-dashed border-accent-line bg-accent-tint px-2 py-1.5">
            <MarkdownBody
              text={capture.body}
              className="text-xs text-neutral-600 dark:text-neutral-300"
            />
            <Mono size="xs" tone="accent" className="mt-1 block">
              SESSION DIGEST
            </Mono>
          </div>
        ) : hasBlob ? (
          <div className="flex flex-col gap-1.5">
            {imageUrl && (
              <img
                src={imageUrl}
                alt="Screenshot capture"
                className="max-h-48 w-fit max-w-full rounded-md border border-neutral-200 object-contain dark:border-neutral-800"
              />
            )}
            {ocrText ? (
              <MarkdownBody
                text={ocrText}
                className="text-xs text-neutral-500 dark:text-neutral-400"
              />
            ) : (
              <Mono size="sm" tone="accent">
                ◐ READING TEXT…
              </Mono>
            )}
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

        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1">
          <p className="text-xs text-neutral-400 dark:text-neutral-500">{timestamp}</p>

          {/* Stream provenance -- source app, project + filing confidence,
              session, merge count. Absent (not shown empty) when the
              underlying field is null -- see the redesign plan's "render
              as unknown, never invent." */}
          {streamRow?.source_app_name && (
            <Mono size="xs" tone="faint">
              {streamRow.source_app_name.toUpperCase()}
            </Mono>
          )}
          {streamRow?.project_name && (
            <Chip variant={streamRow.capture.filed_confidence === "certain" ? "neutral" : "accent"}>
              {streamRow.project_name.toUpperCase()}
              {streamRow.capture.filed_confidence ? ` — ${streamRow.capture.filed_confidence.toUpperCase()}` : ""}
            </Chip>
          )}
          {streamRow?.session_ordinal != null && (
            <Mono size="xs" tone="faint">
              S{streamRow.session_ordinal}
              {streamRow.session_client ? ` · ${streamRow.session_client.toUpperCase()}` : ""}
            </Mono>
          )}
          {streamRow && streamRow.merged_count > 0 && (
            <Chip variant="neutral">MERGED ×{streamRow.merged_count}</Chip>
          )}

          {/* Now-row state chips -- leased/handback only; pinned/done/open
              carry no extra chip beyond the glyph itself. */}
          {nowState === "leased" && leaseLabel && (
            <>
              <Chip variant="accent">{leaseLabel}</Chip>
              {onRevokeLease && (
                <button
                  type="button"
                  onClick={onRevokeLease}
                  className="text-label-sm text-fg-faint hover:text-fg-muted"
                >
                  <Mono size="xs" tone="faint">
                    ⌥⌫ REVOKE
                  </Mono>
                </button>
              )}
            </>
          )}
          {nowState === "pinned" && capture.branch && (
            <Mono size="xs" tone="faint">
              PINNED TO {capture.branch.toUpperCase()}
            </Mono>
          )}
          {nowState === "handback" && (
            <>
              {capture.diff_stat && (
                <Mono size="xs" tone="accent">
                  {capture.diff_stat.split("\n").pop()}
                </Mono>
              )}
              {onOpenReview && (
                <Chip variant="solid" onClick={onOpenReview}>
                  ⏎ REVIEW
                </Chip>
              )}
            </>
          )}
        </div>
      </div>

      <div className="shrink-0">
        <ContextMenuTrigger onOpen={(x, y) => menuOpenRef.current?.(x, y)} />
        <ContextMenu items={buildContextMenuItems()} openRef={menuOpenRef} />
      </div>

      {expanded && (
        <ExpandedCaptureModal
          capture={capture}
          blob={blob}
          imageUrl={imageUrl}
          timestamp={timestamp}
          startEditing={editing}
          onClose={closeExpanded}
          onSave={handleSaveEdit}
        />
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

    items.push({
      label: "Copy",
      onClick: () => (isScreenshot ? api.copyCaptureImage(capture.id) : api.copyCaptureText(capture.id)),
    });
    items.push({ label: `Copy as List${batchSuffix}`, onClick: () => api.copyCapturesAsChecklist(ids) });

    // Same omission pattern as Mark Done/Reopen/Promote/Demote above: when a
    // list (e.g. NowList/DockApp) doesn't wire a handler down at all, the
    // corresponding action is simply absent rather than a permanently
    // disabled dead entry.
    if (onEdit) {
      items.push({ label: "Edit", onClick: () => setEditing(true), disabled: editActionDisabled });
      items.push({
        label: "Edit in New Window",
        onClick: () => openEditWindow(),
        disabled: editActionDisabled,
      });
    }

    items.push({ label: "Expand", onClick: () => setExpanded(true) });

    if (onMerge) {
      items.push({
        label: `Merge Notes${batchSuffix}`,
        onClick: () => onMerge(ids),
        // merge_captures errors below 2 sources (crates/magpie-core/src/merge.rs)
        // -- a lone target id is never a valid merge, batch or not.
        disabled: ids.length < 2,
      });
    }

    if (onMoveProject) {
      items.push({
        label: "Move to Project",
        submenu: [
          { label: "Inbox", onClick: () => onMoveProject(ids, null) },
          ...projects.map((p) => ({ label: p.name, onClick: () => onMoveProject(ids, p.id) })),
        ],
      });
    }

    if (onMoveSection) {
      items.push({
        label: "Move to Section",
        submenu: [
          { label: "None", onClick: () => onMoveSection(ids, null) },
          ...sections.map((s) => ({ label: s.name, onClick: () => onMoveSection(ids, s.id) })),
          { label: "New section…", onClick: () => createAndAssignSection(), disabled: !onCreateSection },
        ],
      });
    }

    if (onDelete) {
      items.push({
        label: `Delete${batchSuffix}`,
        onClick: () => onDelete(ids),
        destructive: true,
      });
    }

    return items;
  }
}
