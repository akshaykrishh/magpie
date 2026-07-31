import { Combine, FileText, FolderInput, FolderOutput, Trash2, X } from "lucide-react";
import { useRef } from "react";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";

interface MergeToolbarProps {
  count: number;
  onMerge: () => void;
  onCopyAsList: () => void;
  onMoveToProject: (projectId: number | null) => void;
  onMoveToSection: (sectionId: number | null) => void;
  onDelete: () => void;
  onClear: () => void;
  projects: { id: number; name: string }[];
  sections: { id: number; name: string }[];
}

const buttonClass =
  "flex items-center gap-1.5 rounded-md px-2.5 py-1 text-slate-teal hover:bg-slate-teal/15 " +
  "dark:text-slate-teal-light dark:hover:bg-slate-teal-light/20";

export function MergeToolbar({
  count,
  onMerge,
  onCopyAsList,
  onMoveToProject,
  onMoveToSection,
  onDelete,
  onClear,
  projects,
  sections,
}: MergeToolbarProps) {
  // Same imperative open-at-click-point pattern CaptureItem's own
  // ContextMenuTrigger/ContextMenu pair already uses (see ContextMenu.tsx) --
  // reusing it here (rather than a native <select> or a bespoke dropdown)
  // means "Move to Project/Section" renders and behaves identically whether
  // it's reached via this toolbar or via a row's right-click menu.
  const projectMenuOpenRef = useRef<((x: number, y: number) => void) | null>(null);
  const sectionMenuOpenRef = useRef<((x: number, y: number) => void) | null>(null);

  if (count === 0) return null;

  function openAt(ref: React.MutableRefObject<((x: number, y: number) => void) | null>) {
    return (e: React.MouseEvent<HTMLButtonElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      ref.current?.(rect.left, rect.bottom);
    };
  }

  // Flat menus (no nested submenu) since the trigger button itself already
  // says "Move to Project"/"Move to Section" -- unlike the row context menu,
  // which nests these under a single "Move to Project"/"Move to Section"
  // parent entry alongside unrelated actions, this bar has a dedicated
  // button per destination-picker so the picker's own items are the whole menu.
  const projectMenuItems: ContextMenuItem[] = [
    { label: "Inbox", onClick: () => onMoveToProject(null) },
    ...projects.map((p) => ({ label: p.name, onClick: () => onMoveToProject(p.id) })),
  ];

  const sectionMenuItems: ContextMenuItem[] = [
    { label: "None", onClick: () => onMoveToSection(null) },
    ...sections.map((s) => ({ label: s.name, onClick: () => onMoveToSection(s.id) })),
  ];

  return (
    <div className="flex items-center justify-between rounded-lg bg-slate-teal/10 px-3 py-2 text-sm dark:bg-slate-teal-light/15">
      <span className="text-slate-teal dark:text-slate-teal-light">{count} selected</span>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onMerge}
          disabled={count < 2}
          className="flex items-center gap-1.5 rounded-md bg-slate-teal px-2.5 py-1 text-white
                     hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-40"
        >
          <Combine size={14} />
          Merge
        </button>
        <button type="button" onClick={onCopyAsList} className={buttonClass}>
          <FileText size={14} />
          Copy as List
        </button>
        <div>
          <button type="button" onClick={openAt(projectMenuOpenRef)} className={buttonClass}>
            <FolderInput size={14} />
            Move to Project
          </button>
          <ContextMenu items={projectMenuItems} openRef={projectMenuOpenRef} />
        </div>
        <div>
          <button type="button" onClick={openAt(sectionMenuOpenRef)} className={buttonClass}>
            <FolderOutput size={14} />
            Move to Section
          </button>
          <ContextMenu items={sectionMenuItems} openRef={sectionMenuOpenRef} />
        </div>
        <button
          type="button"
          onClick={onDelete}
          className="flex items-center gap-1.5 rounded-md px-2.5 py-1 text-red-600 hover:bg-red-500/10
                     dark:text-red-400 dark:hover:bg-red-400/10"
        >
          <Trash2 size={14} />
          Delete
        </button>
        <button
          type="button"
          onClick={onClear}
          className="rounded-md p-1 text-slate-teal hover:bg-slate-teal/15 dark:text-slate-teal-light dark:hover:bg-slate-teal-light/20"
        >
          <X size={14} />
        </button>
      </div>
    </div>
  );
}
