import { useState } from "react";
import {
  CapacityDots,
  Chip,
  DiamondMark,
  Earned,
  FloatingSurface,
  Kbd,
  Mono,
  ProgressBar,
  SheetOverlay,
  StatusGlyph,
  type DiamondMarkState,
  type StatusGlyphState,
} from "./components/ui";
import { applyMode, type ThemeMode } from "./lib/theme";

/// A dev-only page (see gallery.html / gallery.tsx -- a Vite input, not a
/// Tauri window) that renders every shared primitive across both themes
/// and, as later stages build them, every surface across the five data
/// density tiers described in the redesign plan (zero / sparse / working
/// / dense / degenerate). This is what makes "fluid across data density"
/// verifiable rather than aspirational -- see the plan's stage 2 note.
///
/// As of stage 2 this only has the bare primitives (nothing app-shaped
/// exists yet). Each later stage that introduces a new visual surface
/// (capture rows, the session strip, the dock, Across, ...) should add a
/// section here alongside it, ideally seeded across all five tiers via
/// `magpie seed --tier <t>` fixtures rather than one hand-picked example.
export function GalleryApp() {
  const [mode, setMode] = useState<ThemeMode>(() =>
    document.documentElement.classList.contains("dark") ? "dark" : "light",
  );
  const [sheetOpen, setSheetOpen] = useState(false);
  const [earnedOn, setEarnedOn] = useState(false);
  const [markKey, setMarkKey] = useState(0);

  function toggleMode() {
    const next = mode === "dark" ? "light" : "dark";
    applyMode(next);
    setMode(next);
  }

  return (
    <div className="relative min-h-screen bg-ground p-10 text-fg">
      <header className="mb-10 flex items-center justify-between">
        <div>
          <h1 className="font-display text-title font-bold">magpie — design gallery</h1>
          <Mono size="sm" tone="faint">
            Slate / Paper primitives -- {mode}
          </Mono>
        </div>
        <button
          type="button"
          onClick={toggleMode}
          className="rounded-sm bg-accent px-3 py-1.5 text-body-sm text-fg-on-accent"
        >
          Toggle theme
        </button>
      </header>

      <Section title="Mono">
        <Row>
          {(["xs", "sm", "md", "lg"] as const).map((size) => (
            <Mono key={size} size={size}>
              size {size}
            </Mono>
          ))}
        </Row>
        <Row>
          {(["fg", "muted", "faint", "accent", "danger"] as const).map((tone) => (
            <Mono key={tone} tone={tone}>
              tone {tone}
            </Mono>
          ))}
        </Row>
      </Section>

      <Section title="Kbd">
        <Row>
          <Kbd>⌘K</Kbd>
          <Kbd>⏎</Kbd>
          <Kbd>⌥⌫</Kbd>
          <Kbd>⇧⏎ NOW</Kbd>
        </Row>
      </Section>

      <Section title="DiamondMark">
        <Row>
          {(["solid", "hollow", "muted"] as const).map((state: DiamondMarkState) => (
            <div key={state} className="flex flex-col items-center gap-2">
              <DiamondMark state={state} />
              <Mono size="xs">{state}</Mono>
            </div>
          ))}
          <div className="flex flex-col items-center gap-2">
            <DiamondMark key={markKey} state="solid" animateIn />
            <button
              type="button"
              onClick={() => setMarkKey((k) => k + 1)}
              className="text-label-sm text-accent underline"
            >
              replay scale-in
            </button>
          </div>
        </Row>
      </Section>

      <Section title="StatusGlyph (Now row states)">
        <Row>
          {(["open", "leased", "pinned", "handback", "done"] as const).map(
            (state: StatusGlyphState) => (
              <div key={state} className="flex flex-col items-center gap-2">
                <StatusGlyph state={state} />
                <Mono size="xs">{state}</Mono>
              </div>
            ),
          )}
        </Row>
      </Section>

      <Section title="CapacityDots">
        <Row>
          {[0, 1, 5, 7].map((filled) => (
            <div key={filled} className="flex flex-col items-center gap-2">
              <CapacityDots filled={filled} cap={7} />
              <Mono size="xs">
                {filled}/7
              </Mono>
            </div>
          ))}
        </Row>
      </Section>

      <Section title="ProgressBar">
        <div className="flex w-full max-w-md flex-col gap-3">
          {[0, 0.14, 0.71, 1].map((v) => (
            <ProgressBar key={v} value={v} />
          ))}
        </div>
      </Section>

      <Section title="Chip">
        <Row>
          <Chip variant="neutral">MAGPIE-CORE — CERTAIN</Chip>
          <Chip variant="accent">⌥⏎ REFILE</Chip>
          <Chip variant="solid">⏎ REVIEW</Chip>
          <Chip variant="neutral">MERGED ×3</Chip>
        </Row>
      </Section>

      <Section title="FloatingSurface">
        <Row>
          <FloatingSurface weight="float" className="px-4 py-3">
            <Mono tone="fg" wide={false}>
              float weight
            </Mono>
          </FloatingSurface>
          <FloatingSurface weight="sheet" className="px-4 py-3">
            <Mono tone="fg" wide={false}>
              sheet weight
            </Mono>
          </FloatingSurface>
        </Row>
      </Section>

      <Section title="SheetOverlay">
        <button
          type="button"
          onClick={() => setSheetOpen(true)}
          className="rounded-sm border border-hairline-strong px-3 py-1.5 text-body-sm"
        >
          Open sheet
        </button>
        {sheetOpen && (
          <SheetOverlay onDismiss={() => setSheetOpen(false)} panelClassName="w-96 p-6">
            <Mono tone="fg" wide={false} className="block">
              Click outside to dismiss.
            </Mono>
          </SheetOverlay>
        )}
      </Section>

      <Section title="Earned — the surface-visibility gate">
        <Row>
          <button
            type="button"
            onClick={() => setEarnedOn((v) => !v)}
            className="rounded-sm border border-hairline-strong px-3 py-1.5 text-body-sm"
          >
            toggle "has data"
          </button>
          <div className="min-w-40">
            <Earned when={earnedOn} fallback={<Mono tone="faint">(absent -- not rendered)</Mono>}>
              <Chip variant="accent">session strip would render here</Chip>
            </Earned>
          </div>
        </Row>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mb-10 border-b border-hairline pb-8">
      <Mono size="sm" wide className="mb-4 block">
        {title}
      </Mono>
      {children}
    </section>
  );
}

function Row({ children }: { children: React.ReactNode }) {
  return <div className="flex flex-wrap items-center gap-6">{children}</div>;
}
