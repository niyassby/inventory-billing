import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    include: ["packages/*/src/**/*.test.ts", "src/**/*.test.ts"],
    environment: "node",
    coverage: {
      provider: "v8",
      reporter: ["text", "html", "json-summary"],
      // Business-logic modules must stay at or above 80% (Master Prompt §13).
      include: [
        "packages/shared/src/**/*.ts",
        "src/api/**/*.ts",
        "src/lib/format.ts",
        "src/app/nav.ts",
        "src/features/pos/cart.ts",
        "src/features/sales/print.ts",
      ],
      exclude: ["**/*.test.ts", "**/index.ts", "**/generated/**", "**/types/**"],
      thresholds: { lines: 80, functions: 80, branches: 80, statements: 80 },
    },
  },
});
