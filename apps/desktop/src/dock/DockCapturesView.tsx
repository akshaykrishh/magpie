import { emit, listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import { AddPromptInput } from "../components/AddPromptInput";
import { CaptureRow } from "../components/CaptureRow";
import { api } from "../lib/api";
import type { Capture } from "../lib/types";

const RECENT_LIMIT = 30;

/// The "I don't use Now" tab: a flat, reverse-chronological list of
/// everything captured, click-to-copy, with its own search and its own
/// add-input (which captures WITHOUT promoting -- see add_capture). Not
/// project-scoped, unlike Now/Activity -- someone living in this tab
/// probably isn't living in the project-focus model at all.
export function DockCapturesView() {
  const [recent, setRecent] = useState<Capture[]>([]);
  const [query, setQuery] = useState("");
  const [searchResults, setSearchResults] = useState<Capture[] | null>(null);

  const refreshRecent = useCallback(() => {
    // Deliberately `listStream({ kind: "all" }, ...)`, NOT
    // `searchCaptures("", RECENT_LIMIT)`: Store::search's sanitize_query
    // (crates/magpie-core/src/search.rs) returns `None` for an
    // empty/whitespace-only query, and `search()` short-circuits that straight
    // to `Ok(Vec::new())` -- an empty FTS query means "zero rows," not "match
    // everything." `listStream` with the `all` filter is this app's actual
    // "recency across every project" primitive (ORDER BY created_at DESC, id
    // DESC, same merged/deleted exclusions `search` applies) and is what this
    // list needs to ever show anything.
    api
      .listStream({ kind: "all" }, RECENT_LIMIT, 0)
      .then(setRecent)
      .catch(console.error);
  }, []);

  useEffect(() => {
    refreshRecent();
    const unlisten = listen("capture:added", refreshRecent);
    return () => {
      unlisten.then((f) => f());
    };
  }, [refreshRecent]);

  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setSearchResults(null);
      return;
    }
    const timeout = setTimeout(() => {
      api.searchCaptures(trimmed).then(setSearchResults).catch(console.error);
    }, 150);
    return () => clearTimeout(timeout);
  }, [query]);

  async function handleAdd(body: string) {
    await api.addCapture(body);
    refreshRecent();
    // add_capture doesn't emit this server-side (same precedent as
    // add_typed_capture -- see the note above this step's code block),
    // so this window has to tell everyone else a capture landed itself,
    // the same way every other capture surface in the app already does.
    emit("capture:added");
  }

  async function handleCopy(c: Capture) {
    if (c.body) {
      await api.copyCaptureText(c.id);
    } else {
      await api.copyCaptureImage(c.id);
    }
  }

  const items = searchResults ?? recent;

  return (
    <>
      <input
        type="text"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search captures…"
        className="w-full rounded-sm border border-hairline bg-surface px-2 py-1 text-body-sm text-fg placeholder:text-fg-faint"
      />
      <AddPromptInput onAdd={handleAdd} />
      <div className="flex-1 overflow-y-auto">
        {items.length === 0 && (
          <p className="px-1 py-4 text-center text-body-sm text-fg-faint">
            {searchResults !== null ? "No matches." : "Nothing captured yet."}
          </p>
        )}
        {items.map((c) => (
          <div
            key={c.id}
            onClick={(e) => {
              const target = e.target as HTMLElement;
              if (target.closest("button, a, input, textarea")) return;
              void handleCopy(c);
            }}
          >
            <CaptureRow capture={c} compact />
          </div>
        ))}
      </div>
    </>
  );
}
