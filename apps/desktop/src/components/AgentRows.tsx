import { agentName, elapsed } from "@/lib/format";
import type { AgentRow } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Earned, StatusGlyph } from "./ui";

interface AgentRowsProps {
  agents: AgentRow[];
}

/// Every agent starling can see working on the focused project, so the
/// Dock lists the ones that have not called a magpie tool yet alongside the
/// ones that have. Earned: absent without starling, and when nothing is
/// running here. A waiting agent reuses the "needs you" treatment the
/// hand-back row already has (solid accent glyph, accent chip); the rest
/// stay quiet.
export function AgentRows({ agents }: AgentRowsProps) {
  return (
    <Earned when={agents.length > 0}>
      <div className="flex flex-col gap-1">
        {agents.map((a) => (
          <AgentRowItem key={a.starling_session} agent={a} />
        ))}
      </div>
    </Earned>
  );
}

function AgentRowItem({ agent }: { agent: AgentRow }) {
  const known = agent.magpie_session !== null;
  const waiting = agent.state === "waiting";
  const subject = agent.title ?? agent.branch;
  const label = subject ? `${agentName(agent)} · ${subject}` : agentName(agent);
  const since = agent.since ? elapsed(new Date(agent.since).toISOString()).toLowerCase() : null;

  return (
    <div
      title={`${label} (${agent.state}${since ? `, ${since}` : ""})`}
      className={cn(
        "flex items-center gap-2 rounded-md border px-2 py-1.5 text-body-sm",
        // A starling-only agent is drawn lighter than one magpie knows: a
        // dashed hairline, like the human's own S0 card, and fainter text.
        known ? "border-hairline-strong text-fg" : "border-dashed border-hairline text-fg-muted",
      )}
    >
      <StatusGlyph
        state={waiting ? "handback" : agent.state === "working" ? "leased" : "open"}
        size={10}
      />
      {/* The label is the only thing that gives way: it truncates, and the
          status beside it never shrinks or overlaps. */}
      <span className="min-w-0 flex-1 truncate whitespace-nowrap">{label}</span>
      <span
        className={cn(
          "shrink-0 whitespace-nowrap",
          waiting ? "font-medium text-accent" : "text-fg-faint",
        )}
      >
        {waiting ? "waiting" : since}
      </span>
    </div>
  );
}
