import React from "react";
import { cx, cva, RecipeVariantProps } from "styled-system/css";
import { Slot } from "@radix-ui/react-slot";

const textStyles = cva({
  variants: {
    variant: {
      code12Regular: {
        textStyle: "code14Regular"
      },
      label16Medium: {
        textStyle: "label16Medium"
      },
      label16Regular: {
        textStyle: "label16Regular"
      },
      label14Medium: {
        textStyle: "label14Medium"
      },
      label14Regular: {
        textStyle: "label14Regular"
      },
      label12Regular: {
        textStyle: "label12Regular"
      },
      label12Medium: {
        textStyle: "label12Medium"
      },
      paragraph10Regular: {
        textStyle: "paragraph10Regular"
      },
      paragraph12Regular: {
        textStyle: "paragraph12Regular"
      },
      paragraph14Regular: {
        textStyle: "paragraph14Regular"
      },
      paragraph16Regular: {
        textStyle: "paragraph16Regular"
      },
      paragraph16Medium: {
        textStyle: "paragraph16Medium"
      },
      label18Medium: {
        textStyle: "label18Medium"
      },
      headline20Medium: {
        textStyle: "headline20Medium"
      },
      headline24Semibold: {
        textStyle: "headline24Semibold"
      },
      headline20Semibold: {
        textStyle: "headline20Semibold"
      }
    },
    color: {
      unset: {},
      primary: {
        color: "text.primary"
      },
      secondary: {
        color: "text.secondary"
      },
      light: {
        color: "text.light"
      },
      lessContrast: {
        color: "text.less-contrast"
      }
    }
  }
});

export type TextProps = {
  as?: "div" | "span" | "label" | "p";
  asChild?: boolean;
  children?: React.ReactNode;
  className?: string;
} & RecipeVariantProps<typeof textStyles>;

type TextElement = React.ElementRef<"span">;

const Text = React.forwardRef<TextElement, TextProps>((props, forwardedRef) => {
  const {
    children,
    className,
    asChild = false,
    as: Tag = "span",
    variant = "label14Regular",
    color = "unset",
    ...textProps
  } = props;
  return (
    <Slot
      {...textProps}
      ref={forwardedRef}
      className={cx(textStyles({ variant, color }), className)}
    >
      {asChild ? children : <Tag>{children}</Tag>}
    </Slot>
  );
});

Text.displayName = "Text";

export default Text;
