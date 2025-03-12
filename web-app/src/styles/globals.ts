import { defineGlobalStyles } from "@pandacss/dev";

export const globalCss = defineGlobalStyles({
  "html, body": {
    overflow: "hidden",
    width: "100%",
    padding: "0px",
    margin: "0px",
    position: "fixed",
  },
  body: {
    height: "100%",
    margin: "0px",
    minHeight: "100dvh",
    bg: "background.secondary",
    position: "fixed",
    cursor: "default",
    display: "flex",
  },
  ".root": {
    width: "100%",
    height: "100%",
    margin: "0px",
    minHeight: "100dvh",
    bg: "background.secondary",
    position: "fixed",
    cursor: "default",
    display: "flex",
  },
  ".markdown": {
    "*": {
      all: "revert",
    },
    "tr": {
      border: "1px solid",
      padding: "0.5rem",
    },
    "th": {
      border: "1px solid",
      padding: "0.5rem",
    },
    "td": {
      border: "1px solid",
      padding: "0.5rem",
    },
    "table": {
      borderCollapse: "collapse",
    }
  },
});
