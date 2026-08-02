import "./lib/theme-boot";
import React from "react";
import ReactDOM from "react-dom/client";
import { GalleryApp } from "./GalleryApp";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <GalleryApp />
  </React.StrictMode>,
);

// Deliberately no `initThemeSync()` call here, unlike every real window's
// entry point (main.tsx, dock-main.tsx, settings-main.tsx). This page is
// opened directly in a browser at http://localhost:1420/gallery.html (see
// vite.config.ts -- it's a Vite input but NOT a Tauri window), so there is
// no Tauri IPC bridge to call `get_setting` over. GalleryApp's own theme
// toggle calls `applyMode`/`resolveMode` directly (pure DOM/matchMedia,
// no IPC) rather than `setThemePreference`, for the same reason.
