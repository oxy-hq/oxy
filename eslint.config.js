import js from "@eslint/js";
import prettier from "eslint-config-prettier";
import jsxA11y from "eslint-plugin-jsx-a11y";
import pluginPromise from "eslint-plugin-promise";
import react from "eslint-plugin-react";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import sonarjs from "eslint-plugin-sonarjs";
import eslintPluginUnicorn from "eslint-plugin-unicorn";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      "dist",
      "web-app/dist",
      "logs",
      ".prettierrc.js",
      "node_modules",
      "dist-ssr",
      "*.local",
      ".vscode/*",
      "!.vscode/extensions.json",
      ".idea",
      ".DS_Store",
      "*.suo",
      "*.ntvs*",
      "*.njsproj",
      "*.sln",
      "*.sw?",
      "**/styled-system/**",
      "target",
    ],
  },
  {
    extends: [
      js.configs.recommended,
      ...tseslint.configs.recommended,
      prettier,
    ],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
      react,
      unicorn: eslintPluginUnicorn,
      "jsx-a11y": jsxA11y,
      sonarjs,
      "@typescript-eslint": tseslint.plugin,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": [
        "warn",
        { allowConstantExport: true },
      ],
      ...sonarjs.configs.recommended.rules,
      "sonarjs/mouse-events-a11y": "off",
      "sonarjs/todo-tag": "off",
    },
    settings: {
      react: {
        version: "detect",
      },
    },
  },
  pluginPromise.configs["flat/recommended"],
);
