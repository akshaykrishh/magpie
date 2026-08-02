import { useMemo, useState } from "react";
import { Mono } from "@/components/ui";
import { api } from "@/lib/api";
import { usePermissionOffer } from "@/state/usePermissionOffer";
import { useOverlay } from "./useOverlay";

interface PaletteCommand {
  id: string;
  label: string;
  hint?: string;
  /// Present but not runnable -- rendered dimmed with `hint` explaining
  /// why, rather than omitted, per the redesign plan's "discoverability is
  /// paid by enumeration": ⌘K lists every command always, including
  /// unavailable ones with one line saying why.
  disabled?: boolean;
  run: () => void;
}

interface CommandPaletteProps {
  /// How many real projects exist -- Across is only meaningful at ≥2 (see
  /// the redesign plan's earned-surfaces table), so this is what decides
  /// whether its palette entry is runnable or just listed-with-a-reason.
  projectCount: number;
}

/// ⌘K's contents. Verbs only -- search lives in the stream itself (type
/// anywhere in it, no palette involved), and running a template has its
/// own prime real estate in the Now column's empty state (see
/// TemplateRunList) -- see the redesign plan's "search and templates"
/// section for why 3a's single ⌘K field over-compresses three different
/// things into one.
///
/// Opens in a browse state (every command listed, nothing typed yet)
/// rather than an empty input demanding a guessed keyword -- this is what
/// makes it work whether there are 4 commands or 400.
export function CommandPalette({ projectCount }: CommandPaletteProps) {
  const overlay = useOverlay();
  const permissionOffer = usePermissionOffer();
  const [query, setQuery] = useState("");
  const acrossAvailable = projectCount >= 2;

  const commands: PaletteCommand[] = useMemo(
    () => [
      {
        id: "templates",
        label: "Templates",
        hint: "run or manage",
        run: () => overlay.replace({ kind: "templates" }),
      },
      {
        id: "activity",
        label: "Activity",
        hint: "⌘L",
        run: () => overlay.replace({ kind: "activity" }),
      },
      {
        id: "across",
        label: "Across all projects",
        hint: acrossAvailable ? "⌘⌥K" : "needs a second project",
        disabled: !acrossAvailable,
        run: () => {
          if (!acrossAvailable) return;
          // Across is its own OS window (see src-tauri/src/across.rs), not
          // an overlay-stack entry -- ⌘⌥K toggles it directly, so the
          // palette's job here is just to trigger the same path and get
          // out of the way.
          api.toggleAcross().catch(console.error);
          overlay.popAll();
        },
      },
      {
        id: "trash",
        label: "Recently Deleted",
        run: () => overlay.replace({ kind: "trash" }),
      },
      {
        id: "settings",
        label: "Settings…",
        run: () => {
          api.showSettingsWindow().catch(console.error);
          overlay.popAll();
        },
      },
      // Earned (see usePermissionOffer), not disabled-with-a-reason like
      // Across above: below 24 clipboard-only captures there's nothing
      // real to offer yet, so the row is absent entirely rather than
      // dimmed. Once earned it stays listed here even after the stream
      // card's been dismissed -- that's the whole point of the fallback.
      ...(permissionOffer.showInPalette
        ? [
            {
              id: "upgrade-capture",
              label: "Upgrade capture permission",
              hint: "one-key capture",
              run: () => {
                permissionOffer.upgrade();
                overlay.popAll();
              },
            },
          ]
        : []),
    ],
    [overlay, acrossAvailable, permissionOffer],
  );

  const filtered = query.trim()
    ? commands.filter((c) => c.label.toLowerCase().includes(query.trim().toLowerCase()))
    : commands;

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter" && filtered.length > 0) {
      e.preventDefault();
      const first = filtered[0];
      if (!first.disabled) first.run();
    }
  }

  return (
    <div className="flex max-h-[70vh] w-[480px] flex-col">
      <input
        autoFocus
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Search, promote, run a template…"
        className="border-b border-hairline bg-transparent px-4 py-3 text-body text-fg placeholder:text-fg-faint focus:outline-none"
      />
      <div className="flex flex-col overflow-y-auto py-1">
        {filtered.length === 0 ? (
          <p className="px-4 py-3 text-body-sm text-fg-faint">No matching commands.</p>
        ) : (
          filtered.map((c) => (
            <button
              key={c.id}
              type="button"
              disabled={c.disabled}
              onClick={c.run}
              className={
                c.disabled
                  ? "flex items-center justify-between px-4 py-2 text-left text-body-sm text-fg-faint"
                  : "flex items-center justify-between px-4 py-2 text-left text-body-sm text-fg hover:bg-surface-hover"
              }
            >
              <span>{c.label}</span>
              {c.hint && (
                <Mono size="sm" tone="faint">
                  {c.hint}
                </Mono>
              )}
            </button>
          ))
        )}
      </div>
    </div>
  );
}
