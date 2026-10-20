import { fileURLToPath } from "node:url";
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
  // The SDK's manifest contract is bundled from its SOURCE, so `dist/main.mjs`
  // and the standalone binaries carry it and a published `oxyc` depends on no
  // SDK package at run time. Same mapping as `tsconfig.json` `paths`.
  alias: {
    "@oxy-hq/sdk/manifest": fileURLToPath(
      new URL("../typescript/src/manifest.ts", import.meta.url)
    ),
    // `oxyc fn dev` runs a handler on the SDK's testing context, bundled the same way.
    "@oxy-hq/sdk/testing": fileURLToPath(
      new URL("../typescript/src/testing/index.ts", import.meta.url)
    )
  },
  // `npx @oxy-hq/cli` and a global install both exec this file directly.
  banner: {
    js: "#!/usr/bin/env node"
  }
});
