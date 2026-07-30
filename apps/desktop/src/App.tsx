import { emit, listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { AddPromptInput } from "./components/AddPromptInput";
import { AuditView } from "./components/AuditView";
import { CaptureItem } from "./components/CaptureItem";
import { Logo, Wordmark } from "./components/Logo";
import { MergeToolbar } from "./components/MergeToolbar";
import { NowList } from "./components/NowList";
import { PermissionBanner } from "./components/PermissionBanner";
import { SearchBar } from "./components/SearchBar";
import { TemplatesPanel } from "./components/TemplatesPanel";
import { api } from "./lib/api";
import { NOW_CHANGED_EVENT } from "./lib/events";
import type { Capabilities, Capture } from "./lib/types";
import { cn } from "./lib/utils";

// Never on first run -- only once the user has felt the two-keystroke
// friction a few times is the upgrade worth interrupting them for.
const CAPTURES_BEFORE_UPGRADE_OFFER = 3;

type View = "captures" | "templates" | "activity";
const VIEWS: { id: View; label: string }[] = [
  { id: "captures", label: "Captures" },
  { id: "templates", label: "Templates" },
  { id: "activity", label: "Activity" },
];

function App() {
  const [view, setView] = useState<View>("captures");
  const [stream, setStream] = useState<Capture[]>([]);
  const [now, setNow] = useState<Capture[]>([]);
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<Capture[] | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const [permissionBannerDismissed, setPermissionBannerDismissed] = useState(false);

  const refreshStream = useCallback(() => {
    api.listStream({ kind: "all" }).then(setStream).catch(console.error);
  }, []);

  const refreshNow = useCallback(() => {
    api.listNow(null).then(setNow).catch(console.error);
  }, []);

  useEffect(() => {
    refreshStream();
    refreshNow();
    api.captureCapabilities().then(setCapabilities).catch(console.error);

    const unlistenCapture = listen("capture:added", () => {
      refreshStream();
      // Capability state itself doesn't change per-capture, but this is
      // cheap and keeps it correct if the user grants Accessibility while
      // the app is running (macOS doesn't require a relaunch for the
      // AXIsProcessTrusted check to reflect a new grant).
      api.captureCapabilities().then(setCapabilities).catch(console.error);
    });
    // The pinned dock (a separate window) can promote/reorder/complete Now
    // items independently -- this is what keeps this window's copy in sync
    // with changes made over there, and vice versa (see handlers below).
    const unlistenNow = listen(NOW_CHANGED_EVENT, refreshNow);
    return () => {
      unlistenCapture.then((f) => f());
      unlistenNow.then((f) => f());
    };
  }, [refreshStream, refreshNow]);

  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setSearchResults(null);
      return;
    }
    const handle = setTimeout(() => {
      api.searchCaptures(trimmed).then(setSearchResults).catch(console.error);
    }, 150);
    return () => clearTimeout(handle);
  }, [query]);

  function toggleSelect(id: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function handlePromote(id: number) {
    await api.promoteCapture(id);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleAddPrompt(body: string) {
    await api.addTypedCapture(body);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleMerge() {
    if (selected.size < 2) return;
    await api.mergeCaptures(Array.from(selected));
    setSelected(new Set());
    refreshStream();
  }

  async function handleNowReorder(id: number, afterId: number | null) {
    // Optimistic: reflect the new order immediately, then reconcile with
    // the server's actual fractional-index result.
    setNow((prev) => {
      const items = [...prev];
      const from = items.findIndex((c) => c.id === id);
      if (from === -1) return prev;
      const [moved] = items.splice(from, 1);
      const afterIndex = afterId === null ? -1 : items.findIndex((c) => c.id === afterId);
      items.splice(afterIndex + 1, 0, moved);
      return items;
    });
    await api.reorderCapture(id, afterId);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleDone(id: number) {
    await api.markCaptureDone(id);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleDemote(id: number) {
    await api.demoteCapture(id);
    refreshNow();
    refreshStream();
    emit(NOW_CHANGED_EVENT);
  }

  const visibleStream = searchResults ?? stream;

  const showPermissionBanner =
    !permissionBannerDismissed &&
    capabilities?.mode === "clipboard_only" &&
    capabilities.synthesized_copy_available &&
    stream.length >= CAPTURES_BEFORE_UPGRADE_OFFER;

  return (
    <main className="flex h-screen flex-col overflow-hidden">
      <header className="flex shrink-0 items-center gap-1.5 border-b border-neutral-200 px-3 py-2 dark:border-neutral-800">
        <Logo size={18} />
        <Wordmark className="text-sm font-bold text-ink dark:text-neutral-100" />
      </header>
      {showPermissionBanner && (
        <div className="p-3 pb-0">
          <PermissionBanner
            onUpgrade={() => {
              api.openAccessibilitySettings().catch(console.error);
            }}
            onDismiss={() => setPermissionBannerDismissed(true)}
          />
        </div>
      )}
      <div className="flex flex-1 overflow-hidden">
        <aside className="flex w-80 shrink-0 flex-col gap-3 border-r border-neutral-200 p-3 dark:border-neutral-800">
          <h2 className="text-xs font-semibold uppercase tracking-wide text-neutral-400">
            Now
          </h2>
          <AddPromptInput onAdd={handleAddPrompt} />
          <div className="flex-1 overflow-y-auto">
            <NowList
              items={now}
              onReorder={handleNowReorder}
              onDone={handleDone}
              onDemote={handleDemote}
            />
          </div>
        </aside>

        <section className="flex flex-1 flex-col overflow-hidden">
          <nav className="flex gap-1 border-b border-neutral-200 px-3 pt-3 dark:border-neutral-800">
            {VIEWS.map((v) => (
              <button
                key={v.id}
                type="button"
                onClick={() => setView(v.id)}
                className={cn(
                  "rounded-t-md px-3 py-1.5 text-sm",
                  view === v.id
                    ? "border-b-2 border-slate-teal font-medium text-slate-teal dark:border-slate-teal-light dark:text-slate-teal-light"
                    : "text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200",
                )}
              >
                {v.label}
              </button>
            ))}
          </nav>

          {view === "captures" && (
            <div className="flex flex-1 flex-col gap-3 overflow-hidden p-3">
              <SearchBar value={query} onChange={setQuery} />
              <MergeToolbar
                count={selected.size}
                onMerge={handleMerge}
                onClear={() => setSelected(new Set())}
              />
              <div className="flex-1 overflow-y-auto">
                {visibleStream.length === 0 ? (
                  <p className="px-3 py-6 text-center text-sm text-neutral-400 dark:text-neutral-600">
                    {searchResults ? "No matches." : "Nothing captured yet — try the hotkey."}
                  </p>
                ) : (
                  <div className="flex flex-col gap-2">
                    {visibleStream.map((capture) => (
                      <CaptureItem
                        key={capture.id}
                        capture={capture}
                        selected={selected.has(capture.id)}
                        onToggleSelect={toggleSelect}
                        onPromote={capture.queue_pos === null ? handlePromote : undefined}
                      />
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}

          {view === "templates" && (
            <TemplatesPanel
              onInstantiated={() => {
                refreshNow();
                emit(NOW_CHANGED_EVENT);
              }}
            />
          )}

          {view === "activity" && <AuditView />}
        </section>
      </div>
    </main>
  );
}

export default App;
