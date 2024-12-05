module.exports = {
  "**/*.{js,ts,tsx}": ["eslint --fix", "prettier --write"],
  "**/*.{ts,tsx}": ["bash -c 'pnpm turbo lint:ts'"],
  "**/*.rs": ["cargo fmt --check -- "],
  "**/!(json-schemas)/*.{html,json,css,scss,md,mdx}": ["prettier -w"],
};
