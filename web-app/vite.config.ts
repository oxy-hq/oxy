import { resolve } from "path";

import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react-swc";
import { defineConfig } from "vite";
import { nodePolyfills } from 'vite-plugin-node-polyfills'


// https://vitejs.dev/config/
export default defineConfig({
  base: "/",
  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
      "styled-system": resolve(__dirname, "./styled-system"),
    },
  },
  plugins: [react(), tailwindcss(), nodePolyfills({
    overrides: {
      // Since `fs` is not supported in browsers, we can use the `memfs` package to polyfill it.
      fs: 'memfs',
    },
  })],
  publicDir: "public",
  clearScreen: false,
  server: {
    port: 5173,
  },
  build: {
    sourcemap: true
  },
});
