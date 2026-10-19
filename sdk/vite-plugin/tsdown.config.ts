import { fileURLToPath } from "node:url";
import { defineConfig } from "tsdown";

export default defineConfig({
  entry: ["src/index.ts"],
  format: ["cjs", "esm"],
  dts: { tsconfig: "tsconfig.dts.json" },
  sourcemap: true,
  clean: true,
  treeshake: true,
  minify: false,
  outDir: "dist",
  // Vite is a peer dep — never bundle it into the plugin.
  external: ["vite"],
  // The SDK's manifest contract is bundled from its SOURCE, so the published
  // plugin depends on no SDK package at run time. Same mapping as
  // `tsconfig.json` `paths`.
  alias: {
    "@oxy-hq/sdk/manifest": fileURLToPath(new URL("../typescript/src/manifest.ts", import.meta.url))
  },
  banner: {
    js: "// @oxy-hq/vite-plugin"
  },
  target: "node20"
});
