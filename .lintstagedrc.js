export default {
  // JS/TS/TSX: Biome handles lint, format, and Tailwind class sorting
  "**/*.{js,ts,tsx}": ["biome check --write --unsafe"],
  // TypeScript type checking
  "**/*.{ts,tsx}": ["bash -c 'pnpm turbo lint:ts'"],
  // Rust formatting (non-CI only)
  // eslint-disable-next-line no-undef
  ...(process.env.CI ? {} : { "**/*.rs": ["cargo fmt --all --check -- "] }),
  // CSS/JSON/HTML: Biome (with Tailwind v4 support)
  "**/*.{css,json,html}": ["biome check --write"],
  // MD/MDX: markdownlint (lint + auto-fix)
  "**/*.{md,mdx}": ["markdownlint-cli2 --fix"]
};
