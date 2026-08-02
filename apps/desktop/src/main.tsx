import "./lib/theme-boot";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initThemeSync } from "./lib/theme";
import { OverlayProvider } from "./overlays/useOverlay";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <OverlayProvider>
      <App />
    </OverlayProvider>
  </React.StrictMode>,
);

void initThemeSync();
