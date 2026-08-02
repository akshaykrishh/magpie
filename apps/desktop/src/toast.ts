import "./lib/theme-boot";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { initThemeSync } from "./lib/theme";
import "./styles/toast.css";

// `window.__TAURI_INTERNALS__` is the marker every `@tauri-apps/api` call
// reads through -- it's injected by the real Tauri webview before any user
// script runs, and is simply absent when this page is opened directly in
// a browser (which this stage's visual verification does -- see the
// redesign plan's verification section: this window has no React tree a
// browser-only harness like GalleryApp.tsx can render, so previewing its
// states needs its own seam, below). Calling `listen()`/`onFocusChanged()`/
// `initThemeSync()` without that marker throws or rejects, so all three
// are gated behind this check rather than wrapped in a try/catch that
// would mask a real regression.
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// Unlike the other entries, this window is rarely visible but its webview
// stays alive for the whole app session (Tauri shows/hides it, it doesn't
// get recreated per capture) -- so it's worth syncing to a theme change
// made in Settings while it's sitting hidden, even though nobody's
// looking at it in that exact moment. See lib/theme.ts's initThemeSync.
if (inTauri) void initThemeSync();

// Mirrors src-tauri/src/toast.rs's `ToastPayload` and `GuessedDestination`
// exactly. Only two `kind`s exist here on purpose -- the design's third
// visual state ("chord" / "Into Now") belongs to the hold-to-aim gesture,
// which doesn't exist yet (see the redesign plan's stage 9). It is not
// stubbed here ahead of that gesture; when stage 9 adds it, this type and
// the render logic below both grow a case at the same time the gesture
// that constructs it lands, not before.
type GuessedDestination = { project_id: number; project_name: string };
type ToastPayload =
  | {
      kind: "captured";
      capture_id: number;
      destination: GuessedDestination | null;
      first_capture: boolean;
    }
  | { kind: "nothing"; message: string };

const toast = document.getElementById("toast")!;
const mark = document.getElementById("mark")!;
const msg = document.getElementById("msg")!;
const dest = document.getElementById("dest")!;
const destName = document.getElementById("dest-name")!;
const explainer = document.getElementById("explainer")!;

// One 220ms scale-in per NEW toast state, never a re-render of the same
// one -- toast.css's #mark animation only plays on (re-)insertion into the
// DOM, so it's restarted by removing and re-adding the class rather than
// relying on it firing automatically on every render() call.
function replayMarkAnimation() {
  mark.style.animation = "none";
  // Force a reflow so the browser actually forgets the previous animation
  // state before it's re-enabled -- without this the second and later
  // toasts in a row wouldn't replay it at all.
  void mark.offsetWidth;
  mark.style.animation = "";
}

function render(payload: ToastPayload) {
  replayMarkAnimation();

  if (payload.kind === "nothing") {
    toast.classList.add("nothing");
    mark.classList.remove("solid");
    mark.classList.add("muted");
    msg.textContent = payload.message;
    dest.hidden = true;
    explainer.hidden = true;
    return;
  }

  toast.classList.remove("nothing");
  mark.classList.remove("muted");
  mark.classList.add("solid");
  msg.textContent = "Captured";

  if (payload.first_capture) {
    // The explainer replaces the destination row entirely for capture #1
    // -- see capture_flow.rs's FIRST_CAPTURE_VISIBLE_MS.
    dest.hidden = true;
    explainer.hidden = false;
    return;
  }
  explainer.hidden = true;

  if (payload.destination) {
    destName.textContent = payload.destination.project_name;
    dest.hidden = false;
  } else {
    // No project exists yet to guess -- a real state at the earliest data
    // tiers (see GuessedDestination's Rust doc comment), not an error:
    // just "Captured" alone, no arrow, no destination.
    dest.hidden = true;
  }
}

if (inTauri) {
  listen<ToastPayload>("toast:show", (event) => {
    render(event.payload);
  });

  // Sanity check carried over from the M0 spike: prove this window never
  // becomes the OS-level focused window. If it ever does, `focused` flips
  // to true here -- this matters just as much now that there's a second
  // (aim, stage 9) and third (Across, stage 10) non-activating panel
  // reusing the same NSPanel technique; a regression here is a regression
  // in all three.
  getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (focused) {
      console.error("[toast] window took OS focus — non-activating setup failed");
      document.title = "FOCUS STOLEN";
    }
  });
} else {
  // Preview-only fallback, never reachable inside the real app: lets this
  // exact page be opened in a plain browser and driven with
  //   window.dispatchEvent(new CustomEvent("magpie:toast-preview", { detail: payload }))
  // to visually verify all four states without needing a full macOS
  // Tauri build for every markup/CSS change.
  window.addEventListener("magpie:toast-preview", (e) => {
    render((e as CustomEvent<ToastPayload>).detail);
  });
}
