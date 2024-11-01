import { defineConfig } from "vite";
import { resolve } from "path";

import react from "@vitejs/plugin-react-swc";

// https://vitejs.dev/config/
export default defineConfig({
  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
      "styled-system": resolve(__dirname, "./styled-system")
    }
  },
  plugins: [react()]
});
