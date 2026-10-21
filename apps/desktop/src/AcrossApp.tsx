import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { CapacityDots, Chip, Earned, Kbd, Mono, StatusGlyph } from "./components/ui";
import { ResumeCard, type ResumeContext } from "./components/ResumeCard";
import { api } from "./lib/api";
import { STARLING_CHANGED_EVENT } from "./lib/events";
import { fetchResumeContext } from "./lib/resume";
import type { ProjectOverview } from "./lib/types";

const EMPTY_RESUME: ResumeContext = { lastCapture: null, digests: [], unreviewed: [] };

/// '1'..'9' then '0' for indices 0..9, mirroring aim.rs's `digit_for_index`
/// exactly -- both surfaces bind a project/destination to the same
/// physical digit row for the same reason (the tenth slot reads '0', not
/// an out-of-order leading zero).
function digitForIndex(index: number): string {
  return index === 9 ? "0" : String(index + 1);
}

/// The cross-project "what needs a look" rollup (⌘⌥K) -- unlike the
/// toast/aim panels, this window takes real keyboard focus (see
/// src-tauri/src/panels.rs's `KeyableFloatingPanel`), so search-as-you-type
/// and arrow/digit navigation are ordinary DOM keyboard handling here,
/// not Rust-side global shortcuts.
function AcrossApp() {
  const [rows, setRows] = useState<ProjectOverview[]>([]);
  const [query, setQuery] = useState("");
  const [highlighted, setHighlighted] = useState(0);
  const [resume, setResume] = useState<ResumeContext>(EMPTY_RESUME);

  const refresh = useCallback(() => {
    setQuery("");
    setHighlighted(0);
    api.listProjectsOverview().then(setRows).catch(console.error);
  }, []);

  useEffect(() => {
    refresh();
    const unlisten = listen("across:show", refresh);
    // An agent appearing or starting to wait while Across is open: refetch
    // the counts only, without clearing what was typed.
    const unlistenStarling = listen(STARLING_CHANGED_EVENT, () => {
      api.listProjectsOverview().then(setRows).catch(console.error);
    });
    return () => {
      unlisten.then((f) => f());
      unlistenStarling.then((f) => f());
    };
  }, [refresh]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter((r) => r.project_name.toLowerCase().includes(q));
  }, [rows, query]);

  const current = filtered[highlighted];

  useEffect(() => {
    if (!current) {
      setResume(EMPTY_RESUME);
      return;
    }
    let cancelled = false;
    fetchResumeContext(current.project_id).then((ctx) => {
      if (!cancelled) setResume(ctx);
    });
    return () => {
      cancelled = true;
    };
  }, [current]);

  function choose(row: ProjectOverview) {
    api.selectAcrossProject(row.project_id).catch(console.error);
  }

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Escape") {
      e.preventDefault();
      api.hideAcross().catch(console.error);
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setHighlighted((i) => Math.min(i + 1, Math.max(filtered.length - 1, 0)));
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      setHighlighted((i) => Math.max(i - 1, 0));
      return;
    }
    if (e.key === "Enter" && current) {
      e.preventDefault();
      choose(current);
      return;
    }
    // A bare digit (no modifier -- this window takes real key focus, so
    // typing a project name that happens to contain a digit must still
    // work) jumps straight to that slot, matching aim.rs's digit-to-index
    // mapping.
    if (/^[0-9]$/.test(e.key) && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const index = e.key === "0" ? 9 : Number(e.key) - 1;
      const row = filtered[index];
      if (row) {
        e.preventDefault();
        choose(row);
      }
    }
  }

  return (
    <div
      className="flex max-h-[420px] w-[560px] flex-col overflow-hidden rounded-2xl border border-hairline-strong bg-overlay shadow-float backdrop-blur-2xl"
      onBlur={(e) => {
        // Click-away: focus left the whole panel, not just moved between
        // its own children.
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) {
          api.hideAcross().catch(console.error);
        }
      }}
    >
      <input
        autoFocus
        value={query}
        onChange={(e) => {
          setQuery(e.target.value);
          setHighlighted(0);
        }}
        onKeyDown={handleKeyDown}
        placeholder="Jump to a project…"
        className="border-b border-hairline bg-transparent px-4 py-3 text-body text-fg placeholder:text-fg-faint focus:outline-none"
      />
      <div className="flex flex-1 overflow-hidden">
        <div className="flex w-56 flex-none flex-col overflow-y-auto border-r border-hairline py-1">
          {filtered.length === 0 ? (
            <p className="px-3 py-3 text-body-sm text-fg-faint">No matching projects.</p>
          ) : (
            filtered.map((row, i) => (
              <button
                key={row.project_id ?? "inbox"}
                type="button"
                onClick={() => choose(row)}
                onMouseEnter={() => setHighlighted(i)}
                className={`flex items-center gap-2 px-3 py-2 text-left text-body-sm ${
                  i === highlighted ? "bg-accent-tint text-fg" : "text-fg-muted hover:bg-surface-hover"
                }`}
              >
                <Mono size="xs" tone={i === highlighted ? "accent" : "faint"} className="w-3">
                  {digitForIndex(i)}
                </Mono>
                <span className="min-w-0 flex-1 truncate" title={row.project_name}>
                  {row.project_name}
                </span>
                <Earned when={row.now_count > 0}>
                  <CapacityDots filled={Math.min(row.now_count, row.now_cap)} cap={row.now_cap} />
                </Earned>
                {/* The attention dot the hand-back row uses, plus how many
                    agents are waiting; the name above gives way first. */}
                <Earned when={row.agents_waiting_count > 0}>
                  <span
                    className="flex shrink-0 items-center gap-1"
                    title={`${row.agents_waiting_count} needs you`}
                  >
                    <StatusGlyph state="handback" size={8} />
                    <Mono size="xs" tone="accent">
                      {row.agents_waiting_count}
                    </Mono>
                  </span>
                </Earned>
                <Earned when={(row.unfiled_count ?? 0) > 0}>
                  <Mono size="xs" tone="faint" className="ml-auto">
                    {row.unfiled_count} unfiled
                  </Mono>
                </Earned>
              </button>
            ))
          )}
        </div>
        <div className="flex-1 overflow-y-auto">
          <Earned
            when={!!current?.branch || (current?.active_session_count ?? 0) > 0}
          >
            <div className="flex flex-wrap gap-1.5 px-3 pt-3">
              <Earned when={!!current?.branch}>
                <Chip variant="neutral">ON {current?.branch?.toUpperCase()}</Chip>
              </Earned>
              <Earned when={(current?.active_session_count ?? 0) > 0}>
                <span className="rounded-xs border border-hairline-strong px-1.5 py-0.5 text-label text-fg-muted">
                  {current?.active_session_count}{" "}
                  {current?.active_session_count === 1 ? "agent" : "agents"}
                </span>
              </Earned>
              <Earned when={(current?.agents_waiting_count ?? 0) > 0}>
                <span className="rounded-xs border border-accent-line px-1.5 py-0.5 text-label font-medium text-accent">
                  {current?.agents_waiting_count} needs you
                </span>
              </Earned>
            </div>
          </Earned>
          <ResumeCard {...resume} />
        </div>
      </div>
      <div className="flex items-center gap-3 border-t border-hairline px-3 py-2">
        <Mono size="xs" tone="faint" className="flex items-center gap-1">
          <Kbd>↑↓</Kbd> navigate
        </Mono>
        <Mono size="xs" tone="faint" className="flex items-center gap-1">
          <Kbd>⏎</Kbd> jump
        </Mono>
        <Mono size="xs" tone="faint" className="flex items-center gap-1">
          <Kbd>Esc</Kbd> dismiss
        </Mono>
      </div>
    </div>
  );
}

export default AcrossApp;
