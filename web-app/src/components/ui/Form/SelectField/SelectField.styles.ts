import { sva } from "styled-system/css";

export const selectFieldStyles = sva({
  slots: ["trigger", "icon", "value", "content", "viewport", "item"],
  base: {
    trigger: {
      "--default-border-shadow": "0 0 0 1px token(colors.border.light)",

      px: "md",
      py: "sm",
      display: "flex",
      alignItems: "center",
      justifyContent: "space-between",
      cursor: "pointer",
      borderRadius: "rounded",
      textStyle: "label14Regular",
      width: "100%",
      flexShrink: "0",
      // TODO: Confirm with design if this is the max width for form fields
      maxW: "420px",
      outline: "none",

      _disabled: {
        color: "text.secondary",
        bg: "transparent",
        // border
        shadow: "0 0 0 1px token(colors.border.primary)",
        pointerEvents: "none",

        _hover: {
          // border
          shadow: "0 0 0 1px token(colors.border.primary)",
        },
      },
    },
    icon: {
      "[role=select-trigger]:where(:not([data-placeholder])) &": {
        color: "text.secondary",
      },
      "[role=select-trigger]:where(:focus):where(:not([disabled])) &": {
        color: "text.primary",
      },
      "[role=select-trigger]:where([data-state=open]):where(:not([disabled])) &":
        {
          color: "text.primary",
        },
      "[role=select-trigger]:where([disabled]) &": {
        color: "text.secondary",
      },
    },

    content: {
      // TODO: Standardize zIndexes
      zIndex: "103",
      bg: "surface.primary",
      borderRadius: "rounded",
      marginTop: "xs",
      // border and shadow
      shadow: "0 0 0 1px token(colors.border.primary), token(shadows.primary)",
      overflow: "hidden",
      minW: "var(--radix-select-trigger-width)",
      padding: "sm",
      // TODO: Confirm with design if this is the max height for select options container
      maxH: "240px",
    },
    viewport: {
      width: "100%",
      display: "flex",
      flexDir: "column",
    },
    item: {
      display: "flex",
      gap: "xs",
      alignItems: "center",
      minH: "sizes.4xl",
      borderRadius: "rounded",
      padding: "sm",
      textStyle: "label14Regular",
      outline: "none",
      cursor: "pointer",
      color: "text.light",
      _hover: {
        bg: "surface.secondary",
      },
      "&[data-highlighted]": {
        bg: "surface.secondary",
      },
      "&[data-state=checked]": {
        bg: "surface.secondary",
      },
    },
  },
  variants: {
    state: {
      default: {
        trigger: {
          bg: "surface.secondary",
          "&[data-placeholder]": {
            color: "text.secondary",
            "&[data-state=open]": {
              color: "text.primary",
            },
          },

          "&[data-state=open]": {
            shadow: "var(--default-border-shadow)",
          },
          _hover: {
            shadow: "var(--default-border-shadow)",
          },
          _focus: {
            shadow: "var(--default-border-shadow)",
            color: "text.primary",
          },
        },
      },
      error: {
        trigger: {
          color: "text.primary",
          // border and shadow
          shadow: "0 0 0 1px token(colors.border.error), token(shadows.error)",
          bg: "background.secondary",
          "&[data-state=open]": {
            shadow: "var(--default-border-shadow)",
          },
          "&[data-state=closed]": {
            bg: "background.secondary",
          },
          _focus: {
            color: "text.primary",
          },
        },
        icon: {
          color: "text.primary",
          "[role=select-trigger]:where([disabled]) &": {
            color: "text.secondary",
          },
        },
      },
    },
  },
});
