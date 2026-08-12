import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  // Relative asset paths let the finished workbench run from a Python wheel,
  // a desktop bundle, or a regular web server without rebuilding it.
  base: "./",
  plugins: [react()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: true,
  },
  server: {
    proxy: {
      "/api": {
        target: "http://127.0.0.1:8765",
        // M1-05: the backend validates the Host header against its loopback
        // address; the dev proxy must rewrite the host to match.
        changeOrigin: true,
        // M1-05 CSRF guard: a browser attaches `Origin` to POSTs, and the
        // backend only accepts its own loopback origin. The dev UI runs on
        // another port, so rewrite Origin to the API origin like the Host.
        // Production serves the workbench same-origin and is unaffected.
        configure: (proxy) => {
          proxy.on("proxyReq", (proxyReq) => {
            proxyReq.setHeader("Origin", "http://127.0.0.1:8765");
          });
        },
      },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: "./src/test/setup.ts",
    css: true,
  },
});

