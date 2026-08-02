import "./lib/theme-boot";
import React from "react";
import ReactDOM from "react-dom/client";
import AcrossApp from "./AcrossApp";
import { initThemeSync } from "./lib/theme";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AcrossApp />
  </React.StrictMode>,
);

void initThemeSync();
