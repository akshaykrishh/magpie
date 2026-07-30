interface LogoProps {
  size?: number;
  className?: string;
}

// The magpie mark, inlined so it renders crisply at any size and never
// depends on an external asset loading. Ink shapes use currentColor so the
// mark follows whatever text color it's placed in (light/dark), rather
// than hardcoding a fill that would go invisible against a dark nav bar --
// only the wing keeps a fixed brand color, tied to --color-fd-primary so
// it already switches between Slate Teal and Slate Teal Light exactly the
// way the rest of the site's accent color does.
export function Logo({ size = 20, className }: LogoProps) {
  return (
    <svg viewBox="26 28 176 176" width={size} height={size} className={className} aria-hidden="true">
      <circle cx="78" cy="92" r="40" fill="currentColor" />
      <polygon points="104,66 104,86 154,75" fill="currentColor" />
      <polygon points="92,116 190,180 138,180" fill="currentColor" />
      <rect x="63" y="124" width="8" height="46" fill="currentColor" />
      <rect x="88" y="124" width="8" height="46" fill="currentColor" />
      <polygon points="72,84 110,84 98,116 60,116" fill="var(--color-fd-primary)" />
      <circle cx="66" cy="74" r="11" fill="var(--color-fd-background)" />
      <circle cx="66" cy="74" r="4.5" fill="currentColor" />
    </svg>
  );
}
