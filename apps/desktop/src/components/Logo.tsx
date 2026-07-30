interface LogoProps {
  size?: number;
  className?: string;
}

/// The magpie mark, inlined so its fill colors can't disappear behind a
/// missing-asset icon and so it renders crisply at any size.
export function Logo({ size = 20, className }: LogoProps) {
  return (
    <svg
      viewBox="26 28 176 176"
      width={size}
      height={size}
      className={className}
      aria-hidden="true"
    >
      <circle cx="78" cy="92" r="40" fill="#0E0E0E" />
      <polygon points="104,66 104,86 154,75" fill="#0E0E0E" />
      <polygon points="92,116 190,180 138,180" fill="#0E0E0E" />
      <rect x="63" y="124" width="8" height="46" fill="#0E0E0E" />
      <rect x="88" y="124" width="8" height="46" fill="#0E0E0E" />
      <polygon points="72,84 110,84 98,116 60,116" fill="#123A4B" />
      <circle cx="66" cy="74" r="11" fill="#F4F2EE" />
      <circle cx="66" cy="74" r="4.5" fill="#0E0E0E" />
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
