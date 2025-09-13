import { sentryVitePlugin } from "@sentry/vite-plugin";
import { resolve } from "path";

import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react-swc";
import { defineConfig } from "vite";
import { nodePolyfills } from "vite-plugin-node-polyfills";
import { visualizer } from "rollup-plugin-visualizer";

// Shared dependency configuration for both dev optimization and production chunking
const dependencies = {
  // Core React ecosystem - most stable, loaded first
  reactCore: [
    "react",
    "react-dom",
    "react-dom/client", // Added - important for React 18+
    "react-router-dom",
    "react-error-boundary",
  ],

  // React UI components - commonly used together
  reactUI: [
    "react-intersection-observer",
    "react-textarea-autosize",
    "react-window",
    "react-resize-detector",
    "react-resizable-panels",
    "react-hotkeys-hook",
    "react-hook-form",
  ],

  // Radix UI components - heavy UI library, separate chunk
  radixUI: [
    "@radix-ui/primitive",
    "@radix-ui/react-alert-dialog",
    "@radix-ui/react-avatar",
    "@radix-ui/react-checkbox",
    "@radix-ui/react-collapsible",
    "@radix-ui/react-context-menu",
    "@radix-ui/react-dialog",
    "@radix-ui/react-dropdown-menu",
    "@radix-ui/react-label",
    "@radix-ui/react-popover",
    "@radix-ui/react-select",
    "@radix-ui/react-separator",
    "@radix-ui/react-slot",
    "@radix-ui/react-switch",
    "@radix-ui/react-tabs",
    "@radix-ui/react-toast",
    "@radix-ui/react-toggle",
    "@radix-ui/react-toggle-group",
    "@radix-ui/react-tooltip",
    "@radix-ui/react-visually-hidden",
  ],

  // Code editors - large, feature-specific chunk
  editors: [
    "@monaco-editor/react",
    "monaco-editor",
    "monaco-yaml",
    "@uiw/react-codemirror",
    "@uiw/codemirror-themes",
    "@codemirror/lang-python",
    "@codemirror/lang-sql",
    "@codemirror/language",
    "@lezer/highlight",
  ],

  // Data visualization - large but specific use case
  visualization: ["echarts", "@xyflow/react", "elkjs"],

  // Content processing - markdown and syntax highlighting
  contentProcessing: [
    "react-markdown",
    "rehype-raw",
    "rehype-sanitize",
    "remark-directive",
    "remark-gfm",
    "unist-util-visit",
    "react-syntax-highlighter",
    "highlight.js",
    "prism-react-renderer",
  ],

  // Data management - state, queries, tables
  dataVendor: [
    "@tanstack/react-query",
    "@tanstack/react-table",
    "@tanstack/react-virtual",
    "zustand",
    "@duckdb/duckdb-wasm",
  ],

  // UI utilities and theming
  uiUtils: [
    "class-variance-authority",
    "clsx",
    "tailwind-merge",
    "sonner",
    "next-themes",
    "tailwindcss-animate",
    "lucide-react",
  ],

  // Animations and interactions
  animations: [
    "@lottiefiles/react-lottie-player",
    "@formkit/auto-animate",
    "react-day-picker", // Date picker has its own animations
  ],

  // Date and time utilities
  dateUtils: ["dayjs", "date-fns"],

  // Data processing and parsing
  dataProcessing: ["lodash", "papaparse", "nunjucks", "yaml"],

  // Small utilities and helpers
  utils: [
    "usehooks-ts",
    "debounce",
    "invariant",
    "uuid",
    "sort-by",
    "js-cookie", // Browser storage utility
  ],

  // State persistence
  persistence: [
    "persist-and-sync", // Added - missing from original config
  ],

  // Network and external services
  network: ["axios", "@microsoft/fetch-event-source"],

  // Development and polyfills - less critical, can be lazy loaded
  dev: ["dotenv", "memfs"],
};

// Flatten all dependencies for optimizeDeps.include
const allDependencies = Object.values(dependencies).flat();

// https://vitejs.dev/config/
export default defineConfig({
  base: "/",
  optimizeDeps: {
    include: allDependencies,
    // Exclude packages that you're actively developing or that cause issues when pre-bundled
    exclude: [
      // Add any packages here that you don't want pre-bundled
      // For example, if you have local packages or packages with dynamic imports
    ],
  },
  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
      "styled-system": resolve(__dirname, "./styled-system"),
      elkjs: "elkjs/lib/elk.bundled.js",
    },
  },
  plugins: [
    react(),
    tailwindcss(),
    nodePolyfills({
      overrides: {
        // Since `fs` is not supported in browsers, we can use the `memfs` package to polyfill it.
        fs: "memfs",
      },
    }),
    visualizer({
      open: true,
      filename: "bundle-report.html",
      gzipSize: true,
      brotliSize: true,
    }),
    sentryVitePlugin({
      org: process.env.SENTRY_ORG || "oxy-z9",
      project: process.env.VITE_SENTRY_PROJECT || "oxy-frontend",
      authToken: process.env.SENTRY_AUTH_TOKEN,
    }),
  ],
  publicDir: "public",
  clearScreen: false,
  server: {
    port: 5173,
    https: {
      key: "../localhost+2-key.pem",
      cert: "../localhost+2.pem",
    },
    // Enable faster dependency pre-bundling during development
    fs: {
      // Allow serving files from one level up to the project root
      allow: [".."],
    },
    // Warm up frequently used files
    warmup: {
      clientFiles: [
        "./src/main.tsx",
        "./src/App.tsx",
        "./src/components/**/*.tsx",
        "./src/pages/**/*.tsx",
      ],
    },
  },
  build: {
    target: "baseline-widely-available",
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks: {
          "react-vendor": dependencies.reactCore,
          "react-ui": dependencies.reactUI,
          "radix-ui": dependencies.radixUI,
          "editor-vendor": dependencies.editors,
          visualization: dependencies.visualization,
          "content-processing": dependencies.contentProcessing,
          "data-vendor": dependencies.dataVendor,
          "ui-utils": dependencies.uiUtils,
          animations: dependencies.animations,
          "date-utils": dependencies.dateUtils,
          "data-processing": dependencies.dataProcessing,
          "utils-vendor": dependencies.utils,
          persistence: dependencies.persistence,
          "network-vendor": dependencies.network,
          "dev-vendor": dependencies.dev,
        },
      },
    },
  },
});
