import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["src/cli.ts"],
  format: ["esm"],
  dts: false,
  sourcemap: true,
  clean: true,
  minify: false,
  outDir: "dist",
  target: "node20",
  // Ship a real shebang so `pnpm dlx create-oxy-app foo` works.
  banner: {
    js: "#!/usr/bin/env node"
  }
});
