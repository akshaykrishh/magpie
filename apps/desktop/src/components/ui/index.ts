// Shared presentational primitives for the redesign. HARD RULE: nothing in
// this directory may import "@/lib/api" or "@tauri-apps/api" (or anything
// that transitively does). These components are used by the aim panel and
// Across -- new, latency-critical, non-activating windows that must not
// carry the IPC/data-fetching surface just to render a diamond mark or a
// mono label. If a component here ever needs live data, that's a sign it
// belongs in components/ (app-aware) instead, not that the rule should
// bend.
export { CapacityDots } from "./CapacityDots";
export { Chip, type ChipVariant } from "./Chip";
export { DiamondMark, type DiamondMarkState } from "./DiamondMark";
export { Earned } from "./Earned";
export { FloatingSurface } from "./FloatingSurface";
export { Kbd } from "./Kbd";
export { Mono } from "./Mono";
export { ProgressBar } from "./ProgressBar";
export { SheetOverlay } from "./SheetOverlay";
export { StatusGlyph, type StatusGlyphState } from "./StatusGlyph";
