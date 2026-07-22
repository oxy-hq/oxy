import { defineConfig } from "tsdown";

export default defineConfig({
  // Multi-entry: shared modules (the OxyAppProvider context, logger, …)
  // land in a common chunk so `@oxy-hq/sdk` and `@oxy-hq/sdk/shell` see the
  // SAME React context instance. Do not duplicate customer-app code into
  // the shell entry. `src/email.ts` is a separate subpath entry
  // (`@oxy-hq/sdk/email`) keeping the preact email renderer out of the main
  // bundle — see src/email.ts.
  entry: {
    index: "src/index.ts",
    shell: "src/shell/index.ts",
    email: "src/email.ts"
  },
  format: ["cjs", "esm"],
  dts: true,
  css: {
    splitting: false
  },
  sourcemap: true,
  clean: true,
  treeshake: true,
  minify: false,
  // `echarts` is an OPTIONAL peer — the dock's charts use it when the host
  // app has it, else fall back to SVG. Keep it external so the dynamic
  // import stays a runtime lookup instead of being bundled. The email
  // renderer (preact) + React stay external too (peerDependencies).
  external: [
    "@duckdb/duckdb-wasm",
    "@radix-ui/react-tooltip",
    "echarts",
    "preact",
    "preact/jsx-runtime",
    "preact-render-to-string",
    "react",
    "react/jsx-runtime",
    "react-dom"
  ],
  // shell.css is copied by the build script (`cp` after tsdown) — the
  // `copy` option raced the second format pass's clean and the file
  // intermittently vanished from dist.
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
