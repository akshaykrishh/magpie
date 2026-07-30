import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { AddPromptInput } from "./components/AddPromptInput";
import { CaptureItem } from "./components/CaptureItem";
import { MergeToolbar } from "./components/MergeToolbar";
import { NowList } from "./components/NowList";
import { SearchBar } from "./components/SearchBar";
import { api } from "./lib/api";
import type { Capture } from "./lib/types";

function App() {
  const [stream, setStream] = useState<Capture[]>([]);
  const [now, setNow] = useState<Capture[]>([]);
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<Capture[] | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());

  const refreshStream = useCallback(() => {
    api.listStream({ kind: "all" }).then(setStream).catch(console.error);
  }, []);

  const refreshNow = useCallback(() => {
    api.listNow(null).then(setNow).catch(console.error);
  }, []);

  useEffect(() => {
    refreshStream();
    refreshNow();

    const unlisten = listen("capture:added", () => {
      refreshStream();
    });
    return () => {
      unlisten.then((f) => f());
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
  }

  async function handleAddPrompt(body: string) {
    await api.addTypedCapture(body);
    refreshNow();
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
  }

  async function handleDone(id: number) {
    await api.markCaptureDone(id);
    refreshNow();
  }

  async function handleDemote(id: number) {
    await api.demoteCapture(id);
    refreshNow();
    refreshStream();
  }

  const visibleStream = searchResults ?? stream;

  return (
    <main className="flex h-screen overflow-hidden">
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

      <section className="flex flex-1 flex-col gap-3 p-3">
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
      </section>
    </main>
  );
}

export default App;
