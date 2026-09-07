export default {
  // JS/TS/TSX/MJS: Biome handles lint, format, and Tailwind class sorting.
  //
  // `.mjs` is here for the Node ESM helpers under `.github/scripts/` and
  // `scripts/ci/` — `{js,ts,tsx}` does not match it, so those had never been
  // through the pre-commit hook. The GLOB is only half of it: Biome still
  // touches only what `biome.json`'s `files.includes` lists, and
  // `--no-errors-on-unmatched` makes an `.mjs` outside that list a no-op. It is
  // the pair that covers them, not either alone.
  //
  // What that buys differs by where it runs. Locally the husky hook writes the
  // fix into your tree. In CI `fmt-web` runs the same task against a tree the
  // job discards, and `biome check --write` exits 0 after fixing — so CI
  // ENFORCES only error-severity diagnostics, not formatting. Committing
  // through the hook is what keeps these files formatted.
  "**/*.{js,mjs,ts,tsx}": [
    "biome check --write --unsafe --no-errors-on-unmatched",
    "biome format --write --no-errors-on-unmatched"
  ],
  // TypeScript type checking.
  //
  // `lint:ts` is `tsc -b`, NOT `tsc --noEmit`. `web-app/tsconfig.json` is
  // solution-style — `"files": []` plus project references — so `--noEmit`
  // typechecks an empty file list and always passes. This gate silently
  // checked nothing and let a TS2304 and a TS2345 through before anyone
  // noticed. `-b` is the invocation that actually walks the references.
  "**/*.{ts,tsx}": ["bash -c 'pnpm turbo lint:ts'"],
  // Rust formatting (non-CI only)
  // Use a function so lint-staged doesn't append individual file paths,
  // which would bypass Cargo.toml edition detection (causing let-chain errors).
  // eslint-disable-next-line no-undef
  ...(process.env.CI ? {} : { "**/*.rs": () => "cargo fmt --all" }),
  // CSS/JSON/HTML: Biome (with Tailwind v4 support)
  "**/*.{css,json,html}": [
    "biome check --write --no-errors-on-unmatched",
    "biome format --write --no-errors-on-unmatched"
  ]
};
