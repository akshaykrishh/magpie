import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "node:path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "src"),
    },
  },

  build: {
    rollupOptions: {
      input: {
        main: resolve(import.meta.dirname, "index.html"),
        toast: resolve(import.meta.dirname, "toast.html"),
        aim: resolve(import.meta.dirname, "aim.html"),
        across: resolve(import.meta.dirname, "across.html"),
        dock: resolve(import.meta.dirname, "dock.html"),
        settings: resolve(import.meta.dirname, "settings.html"),
        // Dev-only design gallery -- a Vite input so it gets React/Tailwind
        // and hot reload, but deliberately NOT a Tauri window (see
        // src-tauri/tauri.conf.json). Open at http://localhost:1420/gallery.html
        // during `pnpm tauri dev` / `pnpm dev`.
        gallery: resolve(import.meta.dirname, "gallery.html"),
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
