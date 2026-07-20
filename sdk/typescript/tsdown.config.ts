import { defineConfig } from "tsdown";

export default defineConfig({
  // `src/email.ts` is a separate subpath entry (`@oxy-hq/sdk/email`) so the
  // preact email renderer stays out of the main bundle — see src/email.ts.
  entry: ["src/index.ts", "src/email.ts"],
  format: ["cjs", "esm"],
  dts: true,
  css: {
    splitting: false
  },
  sourcemap: true,
  clean: true,
  treeshake: true,
  minify: false,
  // Keep the email renderer + React out of the bundle (peerDependencies).
  external: [
    "@duckdb/duckdb-wasm",
    "preact",
    "preact/jsx-runtime",
    "preact-render-to-string",
    "react",
    "react/jsx-runtime",
    "react-dom"
  ],
  outDir: "dist",
  banner: {
    js: "// @oxy/sdk - TypeScript SDK for Oxy data platform"
  },
  target: false,
  transform: {
    define: {
      "import.meta": "{}"
    }
  }
});
