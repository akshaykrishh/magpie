import { emit, listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { AddPromptInput } from "./components/AddPromptInput";
import { NowList } from "./components/NowList";
import { api } from "./lib/api";
import { NOW_CHANGED_EVENT, SECTIONS_CHANGED_EVENT } from "./lib/events";
import type { Capture, Section } from "./lib/types";

/// The pinned dock: a compact, always-on-top view of Now, meant to sit
/// beside whatever you're actually working in. Deliberately just Now --
/// the stream/search/merge surface lives in the main window, which this
/// is not a replacement for.
function DockApp() {
  const [now, setNow] = useState<Capture[]>([]);
  const [sections, setSections] = useState<Section[]>([]);

  const refreshNow = useCallback(() => {
    api.listNow(null).then(setNow).catch(console.error);
  }, []);

  const refreshSections = useCallback(() => {
    api.listSections().then(setSections).catch(console.error);
  }, []);

  useEffect(() => {
    refreshNow();
    refreshSections();
    const unlisten = listen(NOW_CHANGED_EVENT, refreshNow);
    const unlistenSections = listen(SECTIONS_CHANGED_EVENT, refreshSections);
    return () => {
      unlisten.then((f) => f());
      unlistenSections.then((f) => f());
    };
  }, [refreshNow, refreshSections]);

  async function handleAddPrompt(body: string) {
    await api.addTypedCapture(body);
    refreshNow();
    emit(NOW_CHANGED_EVENT);
  }

  async function handleReorder(id: number, afterId: number | null) {
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
    emit(NOW_CHANGED_EVENT);
  }

  async function handleRenameSection(id: number, name: string) {
    await api.renameSection(id, name);
    refreshSections();
    emit(SECTIONS_CHANGED_EVENT);
  }

  async function handleDeleteSection(id: number) {
    await api.deleteSection(id);
    refreshSections();
    refreshNow();
    emit(SECTIONS_CHANGED_EVENT);
    emit(NOW_CHANGED_EVENT);
  }

  async function handleReorderSection(id: number, afterId: number | null) {
    setSections((prev) => {
      const items = [...prev];
      const from = items.findIndex((s) => s.id === id);
      if (from === -1) return prev;
      const [moved] = items.splice(from, 1);
      const afterIndex = afterId === null ? -1 : items.findIndex((s) => s.id === afterId);
      items.splice(afterIndex + 1, 0, moved);
      return items;
    });
    await api.reorderSection(id, afterId);
    refreshSections();
    emit(SECTIONS_CHANGED_EVENT);
  }

  return (
    <main className="flex h-screen flex-col gap-2 overflow-hidden bg-white/95 p-2 dark:bg-neutral-950/95">
      <AddPromptInput onAdd={handleAddPrompt} />
      <div className="flex-1 overflow-y-auto">
        <NowList
          items={now}
          onReorder={handleReorder}
          onDone={handleDone}
          onDemote={handleDemote}
          sections={sections}
          onRenameSection={handleRenameSection}
          onDeleteSection={handleDeleteSection}
          onReorderSection={handleReorderSection}
        />
      </div>
    </main>
  );
}

export default DockApp;
