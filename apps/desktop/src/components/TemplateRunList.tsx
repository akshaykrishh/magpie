import { Play } from "lucide-react";
import { useEffect, useState } from "react";
import { useOverlay } from "@/overlays/useOverlay";
import { api } from "@/lib/api";
import type { Template } from "@/lib/types";

interface TemplateRunListProps {
  onInstantiated: () => void;
}

/// The run-only half of what used to be the Templates tab (see
/// overlays/TemplatesSheet.tsx for the authoring half) -- rendered in the
/// Now column's empty state, per the redesign plan: you want a template
/// exactly when Now is thin, which is exactly when that column has space
/// for this. A template that needs variables filled in opens the full
/// TemplatesSheet instead of running blind -- this list is deliberately
/// too compact to host a fill-in form.
export function TemplateRunList({ onInstantiated }: TemplateRunListProps) {
  const [templates, setTemplates] = useState<Template[]>([]);
  const overlay = useOverlay();

  useEffect(() => {
    api.listTemplates().then(setTemplates).catch(console.error);
  }, []);

  async function run(template: Template) {
    const variables = await api.getTemplateVariables(template.id);
    if (variables.length > 0) {
      // This list is deliberately too compact to host a fill-in form --
      // open the full sheet (TemplateCard's real variable inputs) instead
      // of running blind or half-building a second form here.
      overlay.push({ kind: "templates" });
      return;
    }
    await api.instantiateTemplate(template.id, null);
    onInstantiated();
  }

  if (templates.length === 0) return null;

  return (
    <div className="flex flex-col gap-1.5">
      <p className="px-1 text-xs font-semibold uppercase tracking-wide text-neutral-400">
        Run a template
      </p>
      {templates.slice(0, 5).map((t) => (
        <button
          key={t.id}
          type="button"
          onClick={() => run(t)}
          className="flex items-center gap-2 rounded-md border border-hairline bg-surface px-2.5 py-1.5 text-left text-sm text-fg-muted hover:border-accent-line hover:bg-accent-tint"
        >
          <Play size={12} className="shrink-0 text-neutral-400" />
          <span className="truncate">{t.title}</span>
        </button>
      ))}
    </div>
  );
}
