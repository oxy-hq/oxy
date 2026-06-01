import react from "@vitejs/plugin-react";
import oxyApp from "@oxy-hq/vite-plugin";
import { defineConfig } from "vite";

// `oxyApp()` handles base path, outDir, manifest validation, manifest
// copy, dev `/api` proxy, and the dev-time `window.__OXY_APP__` shim.
export default defineConfig({
  plugins: [react(), oxyApp()]
});
