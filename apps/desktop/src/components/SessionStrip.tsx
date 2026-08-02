import { useEffect, useRef, useState } from "react";
import { Chip, Earned, Mono } from "@/components/ui";
import { elapsed, sessionLabel } from "@/lib/format";
import type { SessionView } from "@/lib/types";
import { cn } from "@/lib/utils";

interface SessionStripProps {
  sessions: SessionView[];
}

/// The session strip: one card per live agent session plus the synthesized
/// S0 card for the human. Earned on whether any *real* session has ever
/// connected -- S0 alone (present in every `list_sessions_view` response)
/// doesn't count, per the redesign plan's "there is no default focused
/// project" reasoning extended to this surface: an agent connecting is
/// what earns this chrome, not the app simply being open.
export function SessionStrip({ sessions }: SessionStripProps) {
  const hasRealSession = sessions.some((s) => !s.is_synthetic);

  // The one earned animation: the strip's arrival plays the 220ms scale-in
  // exactly once, the first time a real session shows up in this window's
  // lifetime -- never again on subsequent refreshes (a session's counters
  // ticking up must not re-trigger it).
  const hasAnimated = useRef(false);
  const [shouldAnimate, setShouldAnimate] = useState(false);
  useEffect(() => {
    if (hasRealSession && !hasAnimated.current) {
      hasAnimated.current = true;
      setShouldAnimate(true);
    }
  }, [hasRealSession]);

  return (
    <Earned when={hasRealSession}>
      <div
        className={cn(
          "flex items-center gap-2 overflow-x-auto border-b border-hairline px-3 py-2",
          shouldAnimate && "animate-mp-scale-in",
        )}
      >
        {sessions.map((s) => (
          <SessionCard key={s.is_synthetic ? "s0" : `${s.ordinal}-${s.started_at}`} session={s} />
        ))}
      </div>
    </Earned>
  );
}

function SessionCard({ session }: { session: SessionView }) {
  if (session.is_synthetic) {
    const apps = session.top_source_apps.map((a) => a.app_name.toUpperCase()).join(", ");
    return (
      <div className="flex shrink-0 items-center gap-2 rounded-md border border-dashed border-hairline-strong px-2.5 py-1.5">
        <Mono size="xs" tone="fg" weight="bold">
          S0 · YOU
        </Mono>
        <Mono size="xs" tone="faint">
          {session.captures_since ?? 0} CAPTURED
          {apps ? ` · MOSTLY ${apps}` : ""}
        </Mono>
      </div>
    );
  }

  return (
    <div className="flex shrink-0 items-center gap-2 rounded-md border border-hairline-strong px-2.5 py-1.5">
      <Mono size="xs" tone="fg" weight="bold">
        {sessionLabel(session)}
      </Mono>
      {/* Branch is only ever known from a connected agent's own report --
          see the redesign plan's "render as unknown, never invent": no
          git shell-out, no guess from `projects.common_git_dir`. */}
      {session.branch && <Chip variant="neutral">ON {session.branch.toUpperCase()}</Chip>}
      <Mono size="xs" tone="faint">
        {elapsed(session.started_at)}
      </Mono>
      <Mono size="xs" tone="accent">
        {session.holds} HOLDS
      </Mono>
      <Mono size="xs" tone="faint">
        {session.drained} DRAINED
        {session.failed_count > 0 ? ` · ${session.failed_count} FAILED` : ""}
        {session.handback_count > 0 ? ` · ${session.handback_count} HANDED BACK` : ""}
      </Mono>
    </div>
  );
}
