import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

type ToastPayload =
  | { kind: "plain"; message: string }
  | { kind: "guess"; capture_id: number; project_id: number; project_name: string };

const msg = document.getElementById("msg")!;
const dest = document.getElementById("dest")!;
const destName = document.getElementById("dest-name")!;

listen<ToastPayload>("toast:show", (event) => {
  const payload = event.payload;
  if (payload.kind === "plain") {
    msg.textContent = payload.message;
    dest.hidden = true;
  } else {
    msg.textContent = "Captured";
    destName.textContent = payload.project_name;
    dest.hidden = false;
  }
});

// Sanity check for the M0 spike: prove this window never becomes the
// OS-level focused window. If it ever does, `focused` flips to true here.
getCurrentWindow().onFocusChanged(({ payload: focused }) => {
  if (focused) {
    console.error("[M0 spike] toast window took OS focus — non-activating setup failed");
    document.title = "FOCUS STOLEN";
  }
});
