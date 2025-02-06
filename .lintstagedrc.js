export default {
  "**/*.{js,ts,tsx}": ["eslint --fix", "prettier --write"],
  "**/*.{html,css,scss,ts,tsx}": "pnpm dlx prettier --write",
  "**/*.{ts,tsx}": ["bash -c 'pnpm turbo lint:ts'"],
  "**/*.rs": ["cargo fmt --all --check -- "],
  "**/!(json-schemas)/*.{html,json,css,scss,md,mdx}": ["prettier -w"],
};
