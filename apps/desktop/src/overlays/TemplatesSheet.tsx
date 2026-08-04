import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { emit, listen } from "@tauri-apps/api/event";
import { Pencil, Play, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { SECTIONS_CHANGED_EVENT } from "@/lib/events";
import { afterIdFromDragEnd, groupBySection } from "@/lib/grouping";
import type { Project, Section, Template } from "@/lib/types";
import { MarkdownBody } from "@/components/MarkdownBody";
import { SectionHeader } from "@/components/SectionHeader";

interface TemplatesSheetProps {
  onInstantiated: () => void;
  onShowUndo: (message: string, onUndo: () => void) => void;
}

/// The authoring half of what used to be the Templates tab -- see
/// components/TemplateRunList.tsx for the other half (a compact "run one
/// of these" list rendered in the Now column's empty state). Reachable
/// now via ⌘K → Manage templates, since there's no tab bar to hold it.
export function TemplatesSheet({ onInstantiated, onShowUndo }: TemplatesSheetProps) {
  const [templates, setTemplates] = useState<Template[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [sections, setSections] = useState<Section[]>([]);
  const [editing, setEditing] = useState<Template | "new" | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");

  const refresh = () => {
    api.listTemplates().then(setTemplates).catch(console.error);
  };

  const refreshSections = () => {
    api.listSections().then(setSections).catch(console.error);
  };

  useEffect(() => {
    refresh();
    api.listProjects().then(setProjects).catch(console.error);
    refreshSections();

    // Cross-window sync: sections are global, and both the dock and this
    // window's own stream/Now views can rename/reorder/delete one via their
    // own SectionHeader. Must also refetch templates, not just sections --
    // a section delete clears its members' section_id server-side, but a
    // template already in this panel's state still carries the stale id
    // until refetched (same fix App.tsx applies for the stream/Now views).
    const unlistenSections = listen(SECTIONS_CHANGED_EVENT, () => {
      refreshSections();
      refresh();
    });
    return () => {
      unlistenSections.then((f) => f());
    };
  }, []);

  function startEdit(t: Template | "new") {
    setEditing(t);
    setTitle(t === "new" ? "" : t.title);
    setBody(t === "new" ? "" : t.body);
  }

  async function save() {
    if (!title.trim() || !body.trim()) return;
    if (editing === "new") {
      await api.createTemplate(title.trim(), body.trim());
    } else if (editing) {
      await api.updateTemplate(editing.id, title.trim(), body.trim());
    }
    setEditing(null);
    refresh();
  }

  async function remove(id: number) {
    await api.deleteTemplate(id); // now soft-delete (Task 5) -- same call, new semantics
    setTemplates((prev) => prev.filter((t) => t.id !== id));
    onShowUndo("Template deleted.", async () => {
      await api.restoreTemplate(id);
      refresh();
    });
  }

  async function instantiate(
    templateId: number,
    projectId: number | null,
    values?: Record<string, string>,
  ) {
    if (values) {
      await api.instantiateTemplateWithValues(templateId, projectId, values);
    } else {
      await api.instantiateTemplate(templateId, projectId);
    }
    onInstantiated();
  }

  async function assignSection(templateId: number, sectionId: number | null) {
    await api.assignTemplateSection(templateId, sectionId);
    refresh();
  }

  async function renameSection(id: number, name: string) {
    await api.renameSection(id, name);
    refreshSections();
    emit(SECTIONS_CHANGED_EVENT);
  }

  async function deleteSection(id: number) {
    await api.deleteSection(id);
    // Deleting a section only clears its members' section_id -- templates
    // themselves aren't touched, so both lists need a refetch to pick up
    // their new (unsectioned) membership.
    refreshSections();
    refresh();
    emit(SECTIONS_CHANGED_EVENT);
  }

  async function reorderSection(id: number, afterId: number | null) {
    setSections((prev) => {
      const items = [...prev];
      const from = items.findIndex((s) => s.id === id);
      if (from === -1) return prev;
      const [moved] = items.splice(from, 1);
      const afterIndex = afterId === null ? -1 : items.findIndex((s) => s.id === afterId);
      items.splice(afterIndex + 1, 0, moved);
      return items;
    });
    await api.reorderSection(id, afterId);
    refreshSections();
    emit(SECTIONS_CHANGED_EVENT);
  }

  const sectionSensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );

  // Grouped the same way the stream/Now views group captures: section
  // groups render above the plain list, each in the section's own
  // fractional `position` order; templates without a section render exactly
  // as they do today, with no "Unsectioned" header.
  const { bySection, unsectioned } = groupBySection(templates, (t) => t.section_id);
  const visibleSections = sections.filter((s) => bySection.has(s.id));

  function handleSectionDragEnd(event: DragEndEvent) {
    const afterId = afterIdFromDragEnd(visibleSections, event);
    if (afterId === undefined) return;
    reorderSection(Number(event.active.id), afterId);
  }

  return (
    <div className="flex max-h-[70vh] flex-col gap-3 overflow-y-auto p-4">
      <div className="flex items-center justify-between">
        <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-400">
          Templates
        </h2>
        <button
          type="button"
          onClick={() => startEdit("new")}
          className="flex items-center gap-1 rounded-md bg-slate-teal px-2 py-1 text-xs text-white hover:opacity-90"
        >
          <Plus size={14} />
          New
        </button>
      </div>

      {editing && (
        <div className="flex flex-col gap-2 rounded-lg border border-neutral-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-900">
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Title"
            className="rounded-md border border-neutral-200 bg-transparent px-2 py-1 text-sm focus:border-accent-line focus:outline-none dark:border-neutral-700"
          />
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="Prompt body"
            rows={4}
            className="rounded-md border border-neutral-200 bg-transparent px-2 py-1 text-sm focus:border-accent-line focus:outline-none dark:border-neutral-700"
          />
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setEditing(null)}
              className="rounded-md px-2 py-1 text-xs text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={save}
              disabled={!title.trim() || !body.trim()}
              className="rounded-md bg-slate-teal px-2 py-1 text-xs text-white hover:opacity-90 disabled:opacity-40"
            >
              Save
            </button>
          </div>
        </div>
      )}

      {templates.length === 0 && !editing && (
        <p className="px-1 text-sm text-neutral-400 dark:text-neutral-600">
          No templates yet. A template is a prompt you'll want to run again --
          in this project, or several.
        </p>
      )}

      <div className="flex flex-col gap-2">
        {/* Only mounted once a section actually has members -- with no
            sections created, this whole block is absent, not merely empty,
            matching the identical pattern in NowList/App.tsx. */}
        {visibleSections.length > 0 && (
          <DndContext
            sensors={sectionSensors}
            collisionDetection={closestCenter}
            onDragEnd={handleSectionDragEnd}
          >
            <SortableContext items={visibleSections.map((s) => s.id)} strategy={verticalListSortingStrategy}>
              {visibleSections.map((s) => (
                <SortableTemplateSectionGroup
                  key={s.id}
                  section={s}
                  templates={bySection.get(s.id)!}
                  projects={projects}
                  sections={sections}
                  onEdit={startEdit}
                  onDelete={remove}
                  onInstantiate={instantiate}
                  onAssignSection={assignSection}
                  onRenameSection={renameSection}
                  onDeleteSection={deleteSection}
                />
              ))}
            </SortableContext>
          </DndContext>
        )}

        {unsectioned.map((t) => (
          <TemplateCard
            key={t.id}
            template={t}
            projects={projects}
            sections={sections}
            onEdit={() => startEdit(t)}
            onDelete={() => remove(t.id)}
            onInstantiate={(projectId, values) => instantiate(t.id, projectId, values)}
            onAssignSection={(sectionId) => assignSection(t.id, sectionId)}
          />
        ))}
      </div>
    </div>
  );
}

// The template analog of NowList's `SortableSectionGroup`: the header plus
// its member templates render as one draggable unit for section-header
// reordering. Unlike captures, templates have no independent per-item
// ordering to preserve (the library already orders by created_at DESC, not
// reorderable) -- so member templates render in plain order, no nested
// DndContext needed.
function SortableTemplateSectionGroup({
  section,
  templates,
  projects,
  sections,
  onEdit,
  onDelete,
  onInstantiate,
  onAssignSection,
  onRenameSection,
  onDeleteSection,
}: {
  section: Section;
  templates: Template[];
  projects: Project[];
  sections: Section[];
  onEdit: (t: Template) => void;
  onDelete: (id: number) => void;
  onInstantiate: (templateId: number, projectId: number | null, values?: Record<string, string>) => void;
  onAssignSection: (templateId: number, sectionId: number | null) => void;
  onRenameSection: (id: number, name: string) => void;
  onDeleteSection: (id: number) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: section.id,
  });

  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={isDragging ? "opacity-50" : undefined}
    >
      <SectionHeader
        section={section}
        onRename={(name) => onRenameSection(section.id, name)}
        onDelete={() => onDeleteSection(section.id)}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
      <div className="mt-2 flex flex-col gap-2">
        {templates.map((t) => (
          <TemplateCard
            key={t.id}
            template={t}
            projects={projects}
            sections={sections}
            onEdit={() => onEdit(t)}
            onDelete={() => onDelete(t.id)}
            onInstantiate={(projectId, values) => onInstantiate(t.id, projectId, values)}
            onAssignSection={(sectionId) => onAssignSection(t.id, sectionId)}
          />
        ))}
      </div>
    </div>
  );
}

interface VariableMeta {
  description?: string;
  default?: string;
}

function parseVariableMeta(json: string | null): Record<string, VariableMeta> {
  if (!json) return {};
  try {
    return JSON.parse(json);
  } catch {
    return {};
  }
}

function TemplateCard({
  template,
  projects,
  sections,
  onEdit,
  onDelete,
  onInstantiate,
  onAssignSection,
}: {
  template: Template;
  projects: Project[];
  sections: Section[];
  onEdit: () => void;
  onDelete: () => void;
  onInstantiate: (projectId: number | null, values?: Record<string, string>) => void;
  onAssignSection: (sectionId: number | null) => void;
}) {
  const [targetProject, setTargetProject] = useState<string>("");
  // null = not checked yet, [] = checked and has none, string[] = needs filling in.
  const [variables, setVariables] = useState<string[] | null>(null);
  const [values, setValues] = useState<Record<string, string>>({});
  const variableMeta = parseVariableMeta(template.variables_json);

  async function handleRunClick() {
    if (variables === null) {
      const found = await api.getTemplateVariables(template.id);
      if (found.length === 0) {
        onInstantiate(targetProject ? Number(targetProject) : null);
      } else {
        setVariables(found);
      }
      return;
    }
    onInstantiate(targetProject ? Number(targetProject) : null, values);
    setVariables(null);
    setValues({});
  }

  return (
    <div className="flex flex-col gap-2 rounded-lg border border-neutral-200 bg-white p-3 dark:border-neutral-800 dark:bg-neutral-900">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-neutral-800 dark:text-neutral-200">
            {template.title}
          </p>
          {template.description && (
            <p className="mt-0.5 truncate text-xs text-neutral-400 dark:text-neutral-500">
              {template.description}
            </p>
          )}
          <MarkdownBody
            text={template.body}
            className="mt-0.5 line-clamp-2 text-xs text-neutral-500 dark:text-neutral-400"
          />
        </div>
        <div className="flex shrink-0 gap-1">
          <button
            type="button"
            onClick={onEdit}
            className="rounded p-1 text-neutral-400 hover:bg-neutral-100 hover:text-neutral-700 dark:hover:bg-neutral-800"
          >
            <Pencil size={14} />
          </button>
          <button
            type="button"
            onClick={onDelete}
            className="rounded p-1 text-neutral-400 hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-950"
          >
            <Trash2 size={14} />
          </button>
        </div>
      </div>

      {variables && variables.length > 0 && (
        <div className="flex flex-col gap-1.5 rounded-md border border-slate-teal/30 bg-slate-teal/5 p-2 dark:border-slate-teal-light/30 dark:bg-slate-teal-light/10">
          {variables.map((name) => (
            <label key={name} className="flex flex-col gap-0.5 text-xs">
              <span className="text-neutral-500 dark:text-neutral-400">
                {variableMeta[name]?.description ?? name}
              </span>
              <input
                value={values[name] ?? variableMeta[name]?.default ?? ""}
                onChange={(e) => setValues((prev) => ({ ...prev, [name]: e.target.value }))}
                placeholder={name}
                className="rounded-md border border-neutral-200 bg-white px-2 py-1 text-xs focus:border-accent-line focus:outline-none dark:border-neutral-700 dark:bg-neutral-900"
              />
            </label>
          ))}
        </div>
      )}

      <div className="flex items-center gap-2">
        <select
          value={targetProject}
          onChange={(e) => setTargetProject(e.target.value)}
          className="flex-1 rounded-md border border-neutral-200 bg-transparent px-2 py-1 text-xs dark:border-neutral-700"
        >
          <option value="">Inbox (no project)</option>
          {projects.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        {/* "Move to Section" affordance -- functional, not as elaborate as
            CaptureRow's full context menu. A plain select is enough here
            since a template only ever needs to change its single section
            membership, not the richer action set captures get. */}
        {sections.length > 0 && (
          <select
            value={template.section_id ?? ""}
            onChange={(e) => onAssignSection(e.target.value ? Number(e.target.value) : null)}
            className="rounded-md border border-neutral-200 bg-transparent px-2 py-1 text-xs dark:border-neutral-700"
          >
            <option value="">No section</option>
            {sections.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        )}
        {variables && variables.length > 0 && (
          <button
            type="button"
            onClick={() => {
              setVariables(null);
              setValues({});
            }}
            className="rounded-md px-2 py-1 text-xs text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-800"
          >
            Cancel
          </button>
        )}
        <button
          type="button"
          onClick={handleRunClick}
          className="flex items-center gap-1 rounded-md bg-neutral-800 px-2 py-1 text-xs text-white hover:bg-neutral-700 dark:bg-neutral-700 dark:hover:bg-neutral-600"
        >
          <Play size={12} />
          {variables && variables.length > 0 ? "Run with these values" : "Run"}
        </button>
      </div>
    </div>
  );
}
