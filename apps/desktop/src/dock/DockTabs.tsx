export type DockTab = "now" | "captures" | "activity";

const TABS: { key: DockTab; label: string }[] = [
  { key: "now", label: "Now" },
  { key: "captures", label: "Captures" },
  { key: "activity", label: "Activity" },
];

interface DockTabsProps {
  active: DockTab;
  onChange: (tab: DockTab) => void;
}

/// The Dock's 3-way segmented control -- modeled on SettingsApp.tsx's
/// Appearance control (system/light/dark) to keep the same visual language
/// across the app's small chrome surfaces.
export function DockTabs({ active, onChange }: DockTabsProps) {
  return (
    <div className="flex gap-1 rounded-sm border border-hairline bg-surface p-1">
      {TABS.map(({ key, label }) => (
        <button
          key={key}
          type="button"
          onClick={() => onChange(key)}
          aria-pressed={active === key}
          className={
            "flex-1 rounded-xs px-2 py-1 text-body-sm transition-colors " +
            (active === key
              ? "bg-accent text-fg-on-accent"
              : "text-fg-muted hover:bg-surface-hover")
          }
        >
          {label}
        </button>
      ))}
    </div>
  );
}
