import { useEffect, useState } from "react";
import { AddPromptInput } from "@/components/AddPromptInput";
import { NowList } from "@/components/NowList";
import { TemplateRunList } from "@/components/TemplateRunList";
import { Mono, ProgressBar } from "@/components/ui";
import { api } from "@/lib/api";
import type { Capture, Section, Session } from "@/lib/types";
import { useOverlay } from "@/overlays/useOverlay";

interface NowColumnProps {
  now: Capture[];
  sections: Section[];
  onAddPrompt: (body: string) => Promise<void>;
  onReorder: (id: number, afterId: number | null) => void;
  onDone: (id: number) => void;
  onDemote: (id: number) => void;
  onRenameSection: (id: number, name: string) => void;
  onDeleteSection: (id: number) => void;
  onReorderSection: (id: number, afterId: number | null) => void;
  onTemplateInstantiated: () => void;
  /** Refetch `now` after a lease is revoked -- same "mutate then refetch"
      contract as every other handler in this app. */
  onLeaseRevoked: () => void;
}

const DEFAULT_NOW_CAP = 7;

export function NowColumn({
  now,
  sections,
  onAddPrompt,
  onReorder,
  onDone,
  onDemote,
  onRenameSection,
  onDeleteSection,
  onReorderSection,
  onTemplateInstantiated,
  onLeaseRevoked,
}: NowColumnProps) {
  const overlay = useOverlay();
  const [sessions, setSessions] = useState<Session[]>([]);
  const [nowCap, setNowCap] = useState(DEFAULT_NOW_CAP);

  // Sessions have no cross-window "changed" event of their own yet (see
  // the redesign plan's stage 7 -- that's where a real one lands alongside
  // the session strip). Refetching whenever `now` changes is a reasonable
  // proxy: a lease taken/revoked/handed-back always touches a Now item
  // too, so this stays close enough to live without new event plumbing
  // that stage 7 would just replace anyway.
  useEffect(() => {
    api.listSessions(null).then(setSessions).catch(console.error);
  }, [now]);

  useEffect(() => {
    api
      .getSetting("now_cap")
      .then((v) => setNowCap(v ? Number(v) : DEFAULT_NOW_CAP))
      .catch(() => setNowCap(DEFAULT_NOW_CAP));
  }, []);

  async function handleRevokeLease(id: number) {
    await api.revokeLease(id);
    onLeaseRevoked();
  }

  function handleOpenReview(capture: Capture) {
    overlay.push({ kind: "review", capture });
  }

  const leasedCount = now.filter((c) => c.lease_session !== null).length;
  const pinnedCount = now.filter((c) => c.lease_session === null && c.branch !== null).length;
  const fillRatio = nowCap > 0 ? Math.min(now.length, nowCap) / nowCap : 0;

  return (
    <aside className="flex w-80 shrink-0 flex-col gap-3 border-r border-neutral-200 p-3 dark:border-neutral-800">
      <div className="flex flex-col gap-1.5">
        <div className="flex items-baseline justify-between">
          <h2 className="font-display text-lg font-bold text-fg">Now</h2>
          <Mono size="sm" tone="accent">
            {now.length}/{nowCap}
          </Mono>
        </div>
        {now.length > 0 && (
          <>
            <ProgressBar value={fillRatio} />
            <Mono size="xs" tone="faint">
              HUMAN-REVIEWED
              {leasedCount > 0 ? ` · ${leasedCount} LEASED` : ""}
              {pinnedCount > 0 ? ` · ${pinnedCount} OFF-BRANCH` : ""}
            </Mono>
          </>
        )}
      </div>
      <AddPromptInput onAdd={onAddPrompt} />
      {/* You want a template exactly when Now is thin -- see the redesign
          plan's "search and templates" section. Above NowList's own empty
          message, not replacing it, so both read together as one empty
          state rather than the run-list looking like an unrelated
          insertion. */}
      {now.length === 0 && <TemplateRunList onInstantiated={onTemplateInstantiated} />}
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
          onRevokeLease={handleRevokeLease}
          onOpenReview={handleOpenReview}
        />
      </div>
    </aside>
  );
}
