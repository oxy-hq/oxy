export const printWidth = 100;
export const tabWidth = 2;
export const useTabs = false;
export const semi = true;
export const singleQuote = false;
export const jsxSingleQuote = true;
export const trailingComma = "none";
export const bracketSpacing = true;
export const bracketSameLine = false;
export const arrowParens = "always";
export const endOfLine = "crlf";
export const plugins = [
  "@ianvs/prettier-plugin-sort-imports",
  "prettier-plugin-tailwindcss",
];
export const importOrder = [
  "^react$",
  "",
  "<TYPES>",
  "<TYPES>^[.]",
  "",
  "<THIRD_PARTY_MODULES>",
  "",
  "^[@]/",
  "",
  "^[.]",
];
