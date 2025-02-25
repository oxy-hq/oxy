"use client";

import React from "react";
import TextAreaAutosize, {
  TextareaAutosizeProps,
} from "react-textarea-autosize";

import { cx, cva, RecipeVariantProps, css } from "styled-system/css";

const textAreaStyles = cva({
  base: {
    p: "md",
    borderRadius: "rounded",
    resize: "none",
    textStyle: "paragraph14Regular",
    maxW: "420px",
    flexShrink: "0",
    width: "100%",
    outline: "none",

    _disabled: {
      // border
      shadow: "inset 0px 0px 0px 1px token(colors.border.primary)",
      bg: "background.primary",
      color: "text.secondary",
    },
  },
  variants: {
    state: {
      default: {
        bg: "surface.secondary",
        _placeholder: {
          color: "text.secondary",
        },
        _hover: {
          // border
          shadow: "inset 0px 0px 0px 1px token(colors.border.light)",
        },
        _disabled: {
          color: "text.secondary",
          _hover: {
            // border
            shadow: "inset 0 0 0 1px token(colors.border.primary)",
          },
        },
        _focus: {
          // border
          shadow: "inset 0 0 0 1px token(colors.border.light)",
        },
      },
      error: {
        bg: "background.secondary",
        // border and shadow
        shadow:
          "inset 0 0 0 1px token(colors.border.error), token(shadows.error)",
        _focus: {
          // border
          shadow: "inset 0 0 0 1px token(colors.border.light)",
          bg: "surface.secondary",
        },
      },
    },
  },
});

type TextAreaStyledProps = RecipeVariantProps<typeof textAreaStyles>;
export type TextAreaProps = TextareaAutosizeProps & TextAreaStyledProps;

const Textarea = React.forwardRef<HTMLTextAreaElement, TextAreaProps>(
  ({ className, state = "default", ...props }, ref) => {
    return (
      <TextAreaAutosize
        // Need to set height for loading the component in Next.js due to
        // the following issue https://github.com/Andarist/react-textarea-autosize/issues/275
        style={{ height: 104 }}
        minRows={4}
        maxRows={props.maxRows || 4}
        className={cx(
          textAreaStyles({ state }),
          className,
          css({
            customScrollbar: true,
          }),
        )}
        ref={ref}
        {...props}
      />
    );
  },
);

Textarea.displayName = "Textarea";

export default Textarea;
