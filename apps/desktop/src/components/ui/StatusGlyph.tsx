import { cn } from "@/lib/utils";

export type StatusGlyphState = "open" | "leased" | "pinned" | "handback" | "done";

export interface StatusGlyphProps {
  state: StatusGlyphState;
  size?: number;
  className?: string;
}

/// The 12-14px circle glyph carrying a Now row's state -- open, leased,
/// branch-pinned, handed-back, or done. Used in the Now column and the
/// dock, which both render the same five states side by side with the
/// same meaning, so the glyph is one component rather than diverging.
///
///   - open:     hollow ring, neutral -- nothing has happened to it yet
///   - leased:   accent ring + filled accent center (a "held" look,
///               distinct from handback's full fill so a glance tells
///               "someone has this" from "this needs you")
///   - pinned:   dashed neutral ring at reduced opacity -- branch-pinned,
///               visible but explicitly de-emphasized for whoever isn't on
///               that branch
///   - handback: fully solid accent fill -- the one state that means "look
///               at this," so it's the most visually loud of the five
///   - done:     solid but faint/neutral fill -- settled, no longer needs
///               attention, deliberately the visual opposite of handback's
///               vividness despite both being "fully filled"
export function StatusGlyph({ state, size = 14, className }: StatusGlyphProps) {
  const dim = { width: size, height: size };
  const dotInset = Math.max(2, Math.round(size * 0.2));

  if (state === "leased") {
    return (
      <div
        role="presentation"
        style={dim}
        className={cn("relative shrink-0 rounded-full border-[1.5px] border-accent", className)}
      >
        <div
          className="absolute rounded-full bg-accent"
          style={{ inset: dotInset }}
        />
      </div>
    );
  }

  return (
    <div
      role="presentation"
      style={dim}
      className={cn(
        "shrink-0 rounded-full",
        state === "open" && "border-[1.5px] border-hairline-strong",
        state === "pinned" && "border-[1.5px] border-dashed border-hairline-strong opacity-50",
        state === "handback" && "bg-accent",
        state === "done" && "bg-fg-faint",
        className,
      )}
    />
  );
}
