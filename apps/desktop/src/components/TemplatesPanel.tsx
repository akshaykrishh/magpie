import { Pencil, Play, Plus, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { Project, Template } from "@/lib/types";

interface TemplatesPanelProps {
  onInstantiated: () => void;
}

export function TemplatesPanel({ onInstantiated }: TemplatesPanelProps) {
  const [templates, setTemplates] = useState<Template[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [editing, setEditing] = useState<Template | "new" | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");

  const refresh = () => {
    api.listTemplates().then(setTemplates).catch(console.error);
  };

  useEffect(() => {
    refresh();
    api.listProjects().then(setProjects).catch(console.error);
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
    await api.deleteTemplate(id);
    refresh();
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

  return (
    <div className="flex h-full flex-col gap-3 overflow-y-auto p-3">
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
            className="rounded-md border border-neutral-200 bg-transparent px-2 py-1 text-sm focus:border-slate-teal focus:outline-none dark:border-neutral-700"
          />
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="Prompt body"
            rows={4}
            className="rounded-md border border-neutral-200 bg-transparent px-2 py-1 text-sm focus:border-slate-teal focus:outline-none dark:border-neutral-700"
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
        {templates.map((t) => (
          <TemplateCard
            key={t.id}
            template={t}
            projects={projects}
            onEdit={() => startEdit(t)}
            onDelete={() => remove(t.id)}
            onInstantiate={(projectId, values) => instantiate(t.id, projectId, values)}
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
  onEdit,
  onDelete,
  onInstantiate,
}: {
  template: Template;
  projects: Project[];
  onEdit: () => void;
  onDelete: () => void;
  onInstantiate: (projectId: number | null, values?: Record<string, string>) => void;
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
          <p className="mt-0.5 line-clamp-2 text-xs text-neutral-500 dark:text-neutral-400">
            {template.body}
          </p>
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
                className="rounded-md border border-neutral-200 bg-white px-2 py-1 text-xs focus:border-slate-teal focus:outline-none dark:border-neutral-700 dark:bg-neutral-900"
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

