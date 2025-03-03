import React from "react";

import { Slot } from "@radix-ui/react-slot";
import { cva, cx, RecipeVariantProps } from "styled-system/css";

const textStyles = cva({
  variants: {
    variant: {
      code12Regular: {
        textStyle: "code14Regular",
      },
      label16Medium: {
        textStyle: "label16Medium",
      },
      label16Regular: {
        textStyle: "label16Regular",
      },
      label14Medium: {
        textStyle: "label14Medium",
      },
      label14Regular: {
        textStyle: "label14Regular",
      },
      label12Regular: {
        textStyle: "label12Regular",
      },
      label12Medium: {
        textStyle: "label12Medium",
      },
      paragraph10Regular: {
        textStyle: "paragraph10Regular",
      },
      paragraph12Regular: {
        textStyle: "paragraph12Regular",
      },
      paragraph14Regular: {
        textStyle: "paragraph14Regular",
      },
      paragraph16Regular: {
        textStyle: "paragraph16Regular",
      },
      paragraph16Medium: {
        textStyle: "paragraph16Medium",
      },
      label18Medium: {
        textStyle: "label18Medium",
      },
      headline20Medium: {
        textStyle: "headline20Medium",
      },
      headline24Semibold: {
        textStyle: "headline24Semibold",
      },
      headline20Semibold: {
        textStyle: "headline20Semibold",
      },

      // new
      tabBase: {
        textStyle: "tabBase",
      },
      buttonRegular: {
        textStyle: "buttonRegular",
      },
      bodyBaseRegular: {
        textStyle: "bodyBaseRegular",
      },
      bodyBaseMedium: {
        textStyle: "bodyBaseMedium",
      },
      panelTitleRegular: {
        textStyle: "panelTitleRegular",
      },
      body: {
        fontFamily: "Inter",
      },
      panelTitle: {
        fontFamily: "Inter",
        fontSize: "14px",
        fontStyle: "normal",
        lineHeight: "17px",
      },
      button: {
        fontFamily: "Instrument Sans",
        fontSize: "14px",
        fontStyle: "normal",
        fontWeight: 500,
        lineHeight: "150%",
      },
    },
    size: {
      small: {
        fontSize: "12px",
        lineHeight: "14px",
      },
      base: {
        fontSize: "14px",
        lineHeight: "17px",
      },
    },
    weight: {
      regular: {
        fontWeight: 400,
      },
      medium: {
        fontWeight: 500,
      },
      headingH4: {
        textStyle: "headingH4",
      },
    },
    color: {
      unset: {},
      primary: {
        color: "text.primary",
      },
      secondary: {
        color: "text.secondary",
      },
      light: {
        color: "text.light",
      },
      lessContrast: {
        color: "text.less-contrast",
      },
    },
  },
  compoundVariants: [
    {
      variant: "panelTitle",
      weight: "regular",
      css: {
        fontWeight: 600,
      },
    },
    {
      variant: "button",
      weight: "regular",
      css: {
        fontWeight: 500,
      },
    },
  ],
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
      className={cx(textStyles({ variant, color, ...textProps }), className)}
    >
      {asChild ? children : <Tag>{children}</Tag>}
    </Slot>
  );
});

Text.displayName = "Text";

export default Text;
