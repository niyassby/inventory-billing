import { fileURLToPath, URL } from "node:url";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },

  build: {
    rollupOptions: {
      output: {
        // Stable vendor chunks: they cache across app updates and keep the entry small.
        manualChunks(file: string) {
          const id = file.replaceAll("\\", "/");
          if (!id.includes("/node_modules/")) return undefined;
          if (
            /\/node_modules\/(react|react-dom|scheduler|react-router|react-router-dom)\//.test(id)
          )
            return "react";
          if (id.includes("/@tanstack/")) return "tanstack";
          if (/\/node_modules\/(recharts|d3-[^/]+|victory-vendor)\//.test(id)) return "charts";
          if (id.includes("/@base-ui/") || id.includes("/cmdk/") || id.includes("/vaul/"))
            return "ui";
          return undefined;
        },
      },
    },
  },

  // Tauri: keep Rust errors visible and use the fixed port tauri.conf.json expects.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      ignored: ["**/src-tauri/**", "**/coverage/**", "**/.dev-data/**"],
    },
  },
});
