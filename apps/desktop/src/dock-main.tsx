import "./lib/theme-boot";
import React from "react";
import ReactDOM from "react-dom/client";
import DockApp from "./DockApp";
import { initThemeSync } from "./lib/theme";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <DockApp />
  </React.StrictMode>,
);

void initThemeSync();
