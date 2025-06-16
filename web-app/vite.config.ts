import { resolve } from "path";

import tailwindcss from "@tailwindcss/vite"
import react from "@vitejs/plugin-react-swc";
import { defineConfig } from "vite";
import { nodePolyfills } from 'vite-plugin-node-polyfills'
import { visualizer } from 'rollup-plugin-visualizer';


// https://vitejs.dev/config/
export default defineConfig({
  base: "/",
  resolve: {
    alias: {
      "@": resolve(__dirname, "./src"),
      "styled-system": resolve(__dirname, "./styled-system"),
      'elkjs': 'elkjs/lib/elk.bundled.js',
    },
  },
  plugins: [react(), tailwindcss(), nodePolyfills({
    overrides: {
      // Since `fs` is not supported in browsers, we can use the `memfs` package to polyfill it.
      fs: 'memfs',
    },
  }), visualizer({
      open: true,
      filename: 'bundle-report.html',
      gzipSize: true,
      brotliSize: true,
    }),],
  publicDir: "public",
  clearScreen: false,
  server: {
    port: 5173,
  },
  build: {
    sourcemap: true,
    rollupOptions: {
      output: {
        manualChunks: {
          // React ecosystem
          'react-vendor': [
            'react', 
            'react-dom', 
            'react-router-dom',
            'react-error-boundary',
            'react-intersection-observer',
            'react-textarea-autosize',
            'react-window',
            'react-resize-detector',
            'react-resizable-panels'
          ],
          
          // UI libraries
          'radix-ui': [
            '@radix-ui/primitive',
            '@radix-ui/react-alert-dialog',
            '@radix-ui/react-avatar',
            '@radix-ui/react-checkbox',
            '@radix-ui/react-collapsible',
            '@radix-ui/react-context-menu',
            '@radix-ui/react-dialog',
            '@radix-ui/react-dropdown-menu',
            '@radix-ui/react-label',
            '@radix-ui/react-select',
            '@radix-ui/react-separator',
            '@radix-ui/react-slot',
            '@radix-ui/react-switch',
            '@radix-ui/react-tabs',
            '@radix-ui/react-toast',
            '@radix-ui/react-toggle',
            '@radix-ui/react-toggle-group',
            '@radix-ui/react-tooltip',
            '@radix-ui/react-visually-hidden',
          ],

          // Code editors
          'editor-vendor': [
            '@monaco-editor/react',
            'monaco-editor',
            'monaco-yaml',
            '@uiw/react-codemirror',
            '@uiw/codemirror-themes',
            '@codemirror/lang-python',
            '@codemirror/lang-sql',
            '@codemirror/language',
            '@lezer/highlight'
          ],

          // Charts and visualization
          'e-chart-vendor': [
            'echarts'
          ],

          'vega-chart-vendor': [
            'vega',
            'vega-lite',
            'react-vega'
          ],

          // Icons and animations
          'icon-vendor': [
            'lucide-react',
            '@lottiefiles/react-lottie-player',
            '@formkit/auto-animate'
          ],

          // Syntax highlighting and code rendering
          'highlight-vendor': [
            'react-syntax-highlighter',
            'highlight.js',
            'prism-react-renderer'
          ],

          // Markdown and content processing
          'markdown-vendor': [
            'react-markdown',
            'rehype-raw',
            'rehype-sanitize',
            'remark-directive',
            'remark-gfm',
            'unist-util-visit',
          ],

          // UI utilities and styling
          'ui-vendor': [
            'class-variance-authority',
            'clsx',
            'tailwind-merge',
            'sonner',
            'next-themes',
            'tailwindcss-animate'
          ],
          
          // Data flow and state management
          'flow-vendor': ['@xyflow/react'],

          'elkjs': ['elkjs'],
          
          // State management and data fetching
          'state-vendor': [
            '@tanstack/react-query',
            'zustand'
          ],

          // Table and virtualization
          'table-vendor': [
            '@tanstack/react-table',
            '@tanstack/react-virtual'
          ],
          
          // Utilities and helpers
          'utils-vendor': [
            'react-hotkeys-hook',
            'react-hook-form',
            'usehooks-ts',
            'dayjs',
            'debounce',
            'invariant',
            'uuid',
            'sort-by',
            'papaparse'
          ],

          // HTTP and networking
          'network-vendor': [
            'axios',
            '@microsoft/fetch-event-source'
          ],

          // Template engines and parsers
          'template-vendor': [
            'nunjucks',
            'yaml'
          ],

          // Development and build tools
          'dev-vendor': [
            'dotenv',
            'memfs'
          ],

          // Database
          'db-vendor': [
            '@duckdb/duckdb-wasm'
          ]
        }
      }
    }
  },
});
