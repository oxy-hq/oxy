import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["src/index.ts"],
  format: ["cjs", "esm"],
  dts: true,
  css: {
    splitting: false,
  },
  sourcemap: true,
  clean: true,
  treeshake: true,
  minify: false,
  external: ["@duckdb/duckdb-wasm"],
  outDir: "dist",
  banner: {
    js: "// @oxy/sdk - TypeScript SDK for Oxy data platform",
  },
  target: false,
});
