import { AddPromptInput } from "../components/AddPromptInput";
import { AgentRows } from "../components/AgentRows";
import { NowList } from "../components/NowList";
import type { AgentRow, Capture, Section, Session } from "../lib/types";

interface DockNowViewProps {
  now: Capture[];
  sections: Section[];
  sessions: Session[];
  /// Agents on the focused project that starling can see -- empty without
  /// starling.
  agents: AgentRow[];
  onAddPrompt: (body: string) => void;
  onReorder: (id: number, afterId: number | null) => void;
  onDone: (id: number) => void;
  onDemote: (id: number) => void;
  onRevokeLease: (id: number) => void;
  onRenameSection: (id: number, name: string) => void;
  onDeleteSection: (id: number) => void;
  onReorderSection: (id: number, afterId: number | null) => void;
}

/// The Dock's original (and still default) tab: the focused project's Now
/// queue. Extracted unchanged from DockApp.tsx so the tab switcher (added
/// in a later task) has three equally-shaped view components to render.
export function DockNowView({
  now,
  sections,
  sessions,
  agents,
  onAddPrompt,
  onReorder,
  onDone,
  onDemote,
  onRevokeLease,
  onRenameSection,
  onDeleteSection,
  onReorderSection,
}: DockNowViewProps) {
  return (
    <>
      <AgentRows agents={agents} now={now} />
      <AddPromptInput onAdd={onAddPrompt} />
      <div className="flex-1 overflow-y-auto">
        <NowList
          items={now}
          onReorder={onReorder}
          onDone={onDone}
          onDemote={onDemote}
          sections={sections}
          onRenameSection={onRenameSection}
          onDeleteSection={onDeleteSection}
          onReorderSection={onReorderSection}
          sessions={sessions}
          onRevokeLease={onRevokeLease}
          compact
        />
      </div>
    </>
  );
}
