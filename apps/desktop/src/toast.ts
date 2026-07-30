import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

const msg = document.getElementById("msg")!;

listen<string>("toast:show", (event) => {
  msg.textContent = event.payload;
});

// Sanity check for the M0 spike: prove this window never becomes the
// OS-level focused window. If it ever does, `focused` flips to true here.
getCurrentWindow().onFocusChanged(({ payload: focused }) => {
  if (focused) {
    console.error("[M0 spike] toast window took OS focus — non-activating setup failed");
    document.title = "FOCUS STOLEN";
  }
});
