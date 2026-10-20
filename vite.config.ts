import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

// The preview server (used by the end-to-end tests) sends the packaged app's own policy, so a
// change that the policy would block fails in CI instead of in a customer's window.
function previewHeaders(): Record<string, string> {
  const conf = JSON.parse(
    readFileSync(fileURLToPath(new URL("./src-tauri/tauri.conf.json", import.meta.url)), "utf8"),
  ) as { app: { security: { csp: string } } };
  const bridge = process.env.E2E_BRIDGE;
  const csp = bridge
    ? conf.app.security.csp.replace("connect-src 'self'", `connect-src 'self' ${bridge}`)
    : conf.app.security.csp;
  return { "Content-Security-Policy": csp };
}

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

  // Charts load with the dashboard route only; pre-bundling avoids a mid-session reload in dev.
  optimizeDeps: { include: ["recharts"] },

  preview: { headers: previewHeaders() },

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
