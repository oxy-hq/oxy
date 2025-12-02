export default {
  "**/*.{js,ts,tsx}": [
    "eslint --fix --ignore-pattern '**/crates/core/demo_project/**'",
    "prettier --write",
  ],
  "**/*.{ts,tsx}": ["bash -c 'pnpm turbo lint:ts'"],
  // eslint-disable-next-line no-undef
  ...(process.env.CI ? {} : { "**/*.rs": ["cargo fmt --all --check -- "] }),
  "**/!(json-schemas)/*.{html,json,css,scss,md,mdx}": ["prettier -w"],
};
