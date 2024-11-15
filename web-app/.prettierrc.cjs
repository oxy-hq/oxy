module.exports = {
  printWidth: 100,
  tabWidth: 2,
  useTabs: false,
  semi: true,
  singleQuote: false,
  jsxSingleQuote: true,
  trailingComma: "none",
  bracketSpacing: true,
  bracketSameLine: false,
  arrowParens: "always",
  endOfLine: "crlf",
  plugins: ["@ianvs/prettier-plugin-sort-imports", "prettier-plugin-tailwindcss"],
  importOrder: [
    "^react$",
    "",
    "<TYPES>",
    "<TYPES>^[.]",
    "",
    "<THIRD_PARTY_MODULES>",
    "",
    "^[@]/",
    "",
    "^[.]"
  ]
};
