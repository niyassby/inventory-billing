/// <reference types="node" />
import { defineConfig } from "@playwright/test";

/**
 * End-to-end tests drive the real screens in Chrome against the real Rust backend (the dev
 * bridge, started with a fresh demo shop) and the production build served with the same
 * Content-Security-Policy as the packaged app (see vite.config.ts).
 */
const BRIDGE_PORT = 1431;
const APP_PORT = 4173;

export default defineConfig({
  testDir: "e2e",
  globalSetup: "./e2e/global-setup.ts",
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["github"], ["list"]] : "list",
  use: {
    baseURL: `http://127.0.0.1:${APP_PORT}`,
    channel: "chrome",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  webServer: [
    {
      command: `node -e "require('fs').rmSync('.e2e-data',{recursive:true,force:true})" && cargo run --manifest-path src-tauri/Cargo.toml --bin dev-server -- --data .e2e-data --port ${BRIDGE_PORT} --demo`,
      url: `http://127.0.0.1:${BRIDGE_PORT}/health`,
      timeout: 25 * 60_000,
      reuseExistingServer: false,
    },
    {
      command: `npx vite build --outDir dist-e2e --emptyOutDir && npx vite preview --outDir dist-e2e --host 127.0.0.1 --port ${APP_PORT} --strictPort`,
      url: `http://127.0.0.1:${APP_PORT}`,
      timeout: 5 * 60_000,
      reuseExistingServer: false,
      env: {
        VITE_DEV_BRIDGE: `http://127.0.0.1:${BRIDGE_PORT}`,
        E2E_BRIDGE: `http://127.0.0.1:${BRIDGE_PORT}`,
      },
    },
  ],
});
