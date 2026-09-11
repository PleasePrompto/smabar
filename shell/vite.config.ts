import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Serve locales/ from the repo root during dev (source of truth lives outside shell/).
  server: { port: 5173, strictPort: true, fs: { allow: [".."] } },
  clearScreen: false,
  build: {
    rolldownOptions: {
      output: {
        codeSplitting: {
          groups: [{ name: "vendor", test: /node_modules[\\/]/ }],
        },
      },
    },
  },
  // ?inline CSS imports (ui-kit sheet + its anti-drift tests) must yield the
  // real compiled CSS text in vitest, not the default empty stub.
  test: { css: true },
});
