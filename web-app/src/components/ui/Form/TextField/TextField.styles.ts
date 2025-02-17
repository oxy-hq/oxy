import { sva } from "styled-system/css";

export const textFieldStyles = sva({
  slots: ["root", "input", "inputField", "slot"],
  base: {
    root: {
      position: "relative",
      display: "flex",
      // TODO: Confirm with design if this is the max width for form fields
      maxW: "420px",
      alignItems: "center",
      borderRadius: "rounded",
      flexShrink: "0",
      height: "4xl",
      width: "100%",
    },
    input: {
      textStyle: "label14Regular",
      width: "100%",
      outline: "none",
      bg: "transparent",
      zIndex: "1",

      _disabled: {
        color: "text.secondary",
      },
    },
    inputField: {
      position: "absolute",
      inset: "0",
      zIndex: "0",
      pointerEvents: "none",
      borderRadius: "rounded",

      _peerDisabled: {
        // border
        shadow: "inset 0 0 0 1px token(colors.border.primary)",
        bg: "background.primary",
      },
      ".peer:is(:disabled):hover ~ &": {
        // border
        shadow: "inset 0 0 0 1px token(colors.border.primary)",
      },
    },
    slot: {
      display: "flex",
      zIndex: "1",

      _peerDisabled: {
        color: "text.secondary",
      },
    },
  },
  variants: {
    state: {
      default: {
        root: {
          color: "text.primary",
        },
        inputField: {
          bg: "surface.secondary",
          _peerFocus: {
            // border
            shadow: "inset 0 0 0 1px token(colors.border.light)",
          },
          _peerHover: {
            // border
            shadow: "inset 0 0 0 1px token(colors.border.light)",
          },
        },
        input: {
          _placeholder: {
            color: "text.secondary",
          },
        },
        slot: {},
      },
      error: {
        root: {
          color: "text.primary",
        },
        inputField: {
          bg: "surface.secondary",
          // border and shadow
          shadow:
            "inset 0 0 0 1px token(colors.border.error), token(shadows.error)",
          _peerFocus: {
            // border
            shadow: "inset 0 0 0 1px token(colors.border.light)",
          },
        },
        input: {},
        slot: {},
      },
    },
    slotVariant: {
      default: {
        root: {},
        input: {
          py: "sm",
          px: "md",
        },
        inputField: {},
        slot: {
          py: "sm",
          px: "md",
        },
      },
      link: {
        root: {
          // Need a small gap for cosmetic reasons
          gap: "1px",
          px: "md",
        },
        input: {
          py: "sm",
        },
        inputField: {},
        slot: {
          py: "sm",
          color: "text.light",
        },
      },
    },
  },
});
