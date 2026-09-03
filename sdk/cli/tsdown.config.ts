import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["src/main.ts"],
  format: ["esm"],
  dts: false,
  sourcemap: true,
  clean: true,
  minify: false,
  outDir: "dist",
  target: "node20",
  // `npx @oxy-hq/cli` and a global install both exec this file directly.
  banner: {
    js: "#!/usr/bin/env node"
  }
});
