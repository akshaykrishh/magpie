import { elapsed } from "@/lib/format";
import type { AgentRow, Capture } from "@/lib/types";
import { cn } from "@/lib/utils";
import { Earned, StatusGlyph } from "./ui";

interface AgentRowsProps {
  agents: AgentRow[];
  /// The focused project's Now queue, to name the item a connected agent
  /// has taken.
  now: Capture[];
}

/// Every agent starling can see working on the focused project, so the Dock
/// lists the ones that have not called a magpie tool yet alongside the ones
/// that have. Earned: absent without starling, and when nothing is running
/// here.
///
/// A row answers three things in order: what the work is (the item it has
/// taken, else the session's title, else its branch), who is doing it, and
/// whether you need to act ("Needs you" is the only coloured word).
export function AgentRows({ agents, now }: AgentRowsProps) {
  // The most urgent few (the list arrives waiting-first); the rest collapse
  // into one line so a busy project cannot push the queue out of a dock at
  // its minimum height.
  const shown = agents.slice(0, MAX_ROWS);
  const hidden = agents.length - shown.length;
  return (
    <Earned when={agents.length > 0}>
      <div className="flex flex-col gap-1">
        {shown.map((a) => (
          <AgentRowItem key={a.starling_session} agent={a} now={now} />
        ))}
        {hidden > 0 && (
          <span
            className="px-2.5 text-label text-fg-faint"
            title={agents.slice(MAX_ROWS).map((a) => a.title ?? a.branch ?? "Untitled session").join("\n")}
          >
            +{hidden} more
          </span>
        )}
      </div>
    </Earned>
  );
}

const MAX_ROWS = 2;

function firstLine(body: string): string {
  return body.split("\n")[0].trim();
}

function statusText(agent: AgentRow, since: string | null): string {
  switch (agent.state) {
    case "waiting":
      return "Needs you";
    case "working":
      return since ? `Working ${since}` : "Working";
    case "active":
      return since ? `Active ${since}` : "Active";
    default:
      return since ? `Idle ${since}` : "Idle";
  }
}

function AgentRowItem({ agent, now }: { agent: AgentRow; now: Capture[] }) {
  const connected = agent.magpie_session !== null;
  const waiting = agent.state === "waiting";
  const leased = connected
    ? now.find((c) => c.lease_session === agent.magpie_session)
    : undefined;
  const work = (leased && firstLine(leased.body)) || agent.title || agent.branch || "Untitled session";
  const since = agent.since ? elapsed(new Date(agent.since).toISOString()).toLowerCase() : null;
  const status = statusText(agent, since);

  const meta = ["Claude Code"];
  if (agent.branch) meta.push(agent.branch);
  meta.push(connected ? (agent.ordinal != null ? `S${agent.ordinal}` : "connected") : "not connected to magpie");

  return (
    <div
      title={`${work}\n${meta.join(" · ")}\n${status}`}
      className={cn(
        "flex flex-col gap-0.5 rounded-md border px-2.5 py-1.5",
        waiting ? "border-accent-line bg-accent-tint" : "border-hairline",
      )}
    >
      <div className="flex items-center gap-2 text-body">
        <StatusGlyph
          state={waiting ? "handback" : agent.state === "working" ? "leased" : "open"}
          size={10}
        />
        <span className="min-w-0 flex-1 truncate whitespace-nowrap text-fg">{work}</span>
        <span
          className={cn(
            "shrink-0 whitespace-nowrap text-[11px]",
            waiting ? "font-medium text-accent" : "text-fg-faint",
          )}
        >
          {status}
        </span>
      </div>
      <div className="truncate whitespace-nowrap pl-[18px] text-label text-fg-faint">
        {meta.join(" · ")}
      </div>
    </div>
  );
}
