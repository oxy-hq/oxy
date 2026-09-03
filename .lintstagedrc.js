export default {
  // JS/TS/TSX: Biome handles lint, format, and Tailwind class sorting
  // `.mjs` explicitly: `{js,ts,tsx}` does not match it, so no `scripts/ci`
  // script had ever been through the pre-commit hook. The GLOB is half of it —
  // biome still only touches what `biome.json`'s `files.includes` lists, and
  // `--no-errors-on-unmatched` makes the six `.mjs` files outside that list a
  // no-op. It is the pair that covers `scripts/ci`, not either alone.
  "**/*.{js,mjs,ts,tsx}": [
    "biome check --write --unsafe --no-errors-on-unmatched",
    "biome format --write --no-errors-on-unmatched"
  ],
  // TypeScript type checking
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
