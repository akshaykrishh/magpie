import "./lib/theme-boot";
import React from "react";
import ReactDOM from "react-dom/client";
import SettingsApp from "./SettingsApp";
import { initThemeSync } from "./lib/theme";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <SettingsApp />
  </React.StrictMode>,
);

void initThemeSync();
