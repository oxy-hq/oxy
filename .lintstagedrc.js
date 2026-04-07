export default {
  // JS/TS/TSX: Biome handles lint, format, and Tailwind class sorting
  "**/*.{js,ts,tsx}": [
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
  ],
  // MD/MDX: markdownlint (lint + auto-fix)
  "**/*.{md,mdx}": ["markdownlint-cli2 --fix"]
};
