interface LogoProps {
  size?: number;
  className?: string;
}

/// The magpie mark, inlined so its fill colors can't disappear behind a
/// missing-asset icon and so it renders crisply at any size. Every fill is
/// a semantic token, not a hardcoded hex: the body was originally baked in
/// as `--color-ink` (`#0E0E0E`), which is exactly `--mp-ground` on Slate --
/// on a dark-themed title bar (no explicit background of its own, so it
/// shows the page ground straight through) the entire body would vanish
/// into its own backdrop, leaving only a stray wing and eye floating with
/// no bird around them. `fill-fg`/`fill-ground`/`fill-accent` flip with
/// `.dark` the same way every other surface does -- see the redesign
/// plan's stage 12 guardrail, which is what caught this.
export function Logo({ size = 20, className }: LogoProps) {
  return (
    <svg
      viewBox="26 28 176 176"
      width={size}
      height={size}
      className={className}
      aria-hidden="true"
    >
      <circle cx="78" cy="92" r="40" className="fill-fg" />
      <polygon points="104,66 104,86 154,75" className="fill-fg" />
      <polygon points="92,116 190,180 138,180" className="fill-fg" />
      <rect x="63" y="124" width="8" height="46" className="fill-fg" />
      <rect x="88" y="124" width="8" height="46" className="fill-fg" />
      <polygon points="72,84 110,84 98,116 60,116" className="fill-accent" />
      <circle cx="66" cy="74" r="11" className="fill-ground" />
      <circle cx="66" cy="74" r="4.5" className="fill-fg" />
    </svg>
  );
}

export function Wordmark({ className }: { className?: string }) {
  return (
    <span
      className={className}
      style={{ fontFamily: "var(--font-display)", letterSpacing: "-0.035em" }}
    >
      magpie
    </span>
  );
}
