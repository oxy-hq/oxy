import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["src/index.ts"],
  format: ["cjs", "esm"],
  dts: true,
  sourcemap: true,
  clean: true,
  treeshake: true,
  minify: false,
  outDir: "dist",
  // Vite is a peer dep — never bundle it into the plugin.
  external: ["vite"],
  banner: {
    js: "// @oxy-hq/vite-plugin"
  },
  target: "node20"
});
