export default {
  "**/*.{js,ts,tsx}": ["eslint --fix", "prettier --write"],
  "**/*.{ts,tsx}": ["bash -c 'pnpm turbo lint:ts'"],
  "**/*.rs": ["cargo fmt --all --check -- "],
  "**/!(json-schemas)/*.{html,json,css,scss,md,mdx}": ["prettier -w"],
  "**/*.{mdx,md}": [
    "eslint --fix",
    "remark --output --ignore-path .remarkignore --",
  ],
};
