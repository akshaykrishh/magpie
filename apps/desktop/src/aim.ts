import "./lib/theme-boot";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { initThemeSync } from "./lib/theme";
import "./styles/aim.css";

// Same reasoning as toast.ts: `__TAURI_INTERNALS__` is absent when this
// page is opened directly in a browser, which is exactly how this
// window's states get previewed without a full macOS Tauri build for
// every markup/CSS change (see the `magpie:aim-preview` fallback below).
const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

if (inTauri) void initThemeSync();

// Mirrors src-tauri/src/aim.rs's `AimCandidate` exactly.
type AimCandidate = { project_id: number; project_name: string; digit: string };
type AimShowPayload = { candidates: AimCandidate[]; selected_index: number };
type AimSelectPayload = { selected_index: number };

const list = document.getElementById("candidates")!;

let candidates: AimCandidate[] = [];
let rows: HTMLDivElement[] = [];

function render(payload: AimShowPayload) {
  candidates = payload.candidates;
  list.innerHTML = "";
  rows = candidates.map((c) => {
    const row = document.createElement("div");
    row.className = "row";
    const kbd = document.createElement("span");
    kbd.className = "digit";
    kbd.textContent = c.digit;
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = c.project_name;
    row.append(kbd, name);
    list.appendChild(row);
    return row;
  });
  select(payload.selected_index);
}

function select(index: number) {
  rows.forEach((row, i) => row.classList.toggle("selected", i === index));
}

if (inTauri) {
  listen<AimShowPayload>("aim:show", (event) => render(event.payload));
  listen<AimSelectPayload>("aim:select", (event) => select(event.payload.selected_index));

  // Same non-activation sanity check as toast.ts -- a regression here is a
  // regression in every panel sharing panels.rs's NSPanel technique.
  getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (focused) {
      console.error("[aim] window took OS focus — non-activating setup failed");
      document.title = "FOCUS STOLEN";
    }
  });
} else {
  // Preview-only fallback, never reachable inside the real app -- see
  // toast.ts's identical seam:
  //   window.dispatchEvent(new CustomEvent("magpie:aim-preview", { detail: payload }))
  window.addEventListener("magpie:aim-preview", (e) => {
    render((e as CustomEvent<AimShowPayload>).detail);
  });
}
