import { defineConfig } from "@pandacss/dev";

import { globalCss } from "./src/styles/globals";
import { theme } from "./src/styles/theme";
import { extendedUtils } from "./src/styles/utils";

export default defineConfig({
  globalCss,
  // Empty presets to disable all pandacss themes and tokens
  presets: [],
  // Whether to use css reset
  preflight: true,

  // Where to look for css declarations
  include: ["./src/**/*.{js,jsx,ts,tsx}"],

  // Files to exclude
  exclude: [],

  conditions: {
    light: "[data-theme=light] &",
    dark: "[data-theme=dark] &",
    oldTheme: "[data-theme-variant=old] &",
    newTheme: "[data-theme-variant=new] &"
  },

  jsxFramework: "react",
  outdir: "./styled-system",
  theme,

  utilities: {
    // @ts-expect-error Using `var()` declarations in utils is supported but not typed
    extend: extendedUtils
  }
});
