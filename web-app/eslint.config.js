import js from '@eslint/js'
import globals from 'globals'
import reactHooks from 'eslint-plugin-react-hooks'
import reactRefresh from 'eslint-plugin-react-refresh'
import prettier from 'eslint-config-prettier'
import tseslint from 'typescript-eslint'
import react from 'eslint-plugin-react'
import pluginPromise from 'eslint-plugin-promise'
import sonarjs from "eslint-plugin-sonarjs";
import eslintPluginUnicorn from 'eslint-plugin-unicorn';
import jsxA11y from 'eslint-plugin-jsx-a11y';

export default tseslint.config(
  { ignores: ['dist', 'logs','.prettierrc.cjs', 'node_modules', 'dist', 'dist-ssr', '*.local', '.vscode/*', '!.vscode/extensions.json', '.idea', '.DS_Store', '*.suo', '*.ntvs*', '*.njsproj', '*.sln', '*.sw?'] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended, prettier],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    plugins: {
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
      react,
      unicorn: eslintPluginUnicorn,
      'jsx-a11y': jsxA11y,
      sonarjs,
      '@typescript-eslint': tseslint.plugin,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      'react-refresh/only-export-components': [
        'warn',
        { allowConstantExport: true },
      ],
      ...sonarjs.configs.recommended.rules,
      'sonarjs/mouse-events-a11y': 'off',
    },
    settings: {
      react: {
        version: 'detect',
      }
    }
  },
  pluginPromise.configs['flat/recommended'],
)
