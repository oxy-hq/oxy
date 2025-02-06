"use client";

import React from "react";

import type { RecipeVariantProps } from "styled-system/css";

import { Slot } from "@radix-ui/react-slot";
import { cva, cx } from "styled-system/css";

const buttonStyles = cva({
  base: {
    textStyle: "label12Regular",
    display: "inline-flex",
    alignItems: "center",
    cursor: "pointer",
    outline: "none",
    _disabled: {
      pointerEvents: "none",
    },
  },
  variants: {
    size: {
      small: {
        borderRadius: "minimal",
        maxH: "lg",
      },
      medium: {
        borderRadius: "minimal",
        maxH: "2xl",
      },
      large: {
        borderRadius: "rounded",
        maxH: "4xl",
      },
    },
    variant: {
      primary: {
        bg: "surface.contrast",
        color: "text.contrast",
        _hover: {
          bg: {
            base: "dark-grey.opacity",
            _newTheme: "dark-grey-new.opacity",
          },
        },
        _disabled: {
          bg: "surface.contrast",
          color: "text.contrast",
          opacity: "0.2",
        },
      },
      filled: {
        bg: "surface.secondary",
        color: "text.light",
        _hover: {
          bg: "surface.tertiary",
          color: "text.primary",
        },
        _disabled: {
          bg: "surface.primary",
          color: "text.disabled",
        },
      },
      negative: {
        bg: "error.default",
        color: "text.contrast",
        _hover: {
          bg: "error.hover",
        },
        _disabled: {
          bg: "error.background",
          color: "text.contrast",
        },
      },
      outline: {
        color: "text.light",
        // outline
        boxShadow: "inset 0 0 0 1px token(colors.border.primary)",
        _hover: {
          bg: "surface.secondary",
          color: "text.light",
        },
        _disabled: {
          bg: "surface.primary",
          color: "text.disabled",
        },
      },
      ghost: {
        bg: "transparent",
        color: "text.secondary",
        _hover: {
          bg: "surface.secondary",
          color: "text.light",
        },
        _disabled: {
          color: "text.disabled",
        },
        // Used for context menus
        "&[data-state=open]": {
          bg: "surface.secondary",
          color: "text.light",
        },
      },
      transparent: {
        bg: "transparent",
        color: "text.secondary",
        _hover: {
          bg: "surface.primary",
          color: "text.light",
        },
        _disabled: {
          color: "text.disabled",
        },
        // Used for context menus
        "&[data-state=open]": {
          bg: "surface.secondary",
          color: "text.light",
        },
      },
    },
    content: {
      icon: {},
      text: {},
      iconText: {},
    },
  },
  compoundVariants: [
    {
      size: "small",
      content: "text",
      css: {
        p: "sm",
        textStyle: "label12Regular",
      },
    },
    {
      size: "small",
      content: "icon",
      css: {
        p: "xs",
        maxW: "lg",
      },
    },
    {
      size: "small",
      content: "iconText",
      css: {
        py: "xs",
        pl: "sm",
        pr: "md",
        textStyle: "label12Regular",
        gap: "xs",
      },
    },
    {
      size: "medium",
      content: "text",
      css: {
        p: "sm",
        textStyle: "label12Regular",
      },
    },
    {
      size: "medium",
      content: "icon",
      css: {
        p: "xs",
        maxW: "2xl",
      },
    },
    {
      size: "medium",
      content: "iconText",
      css: {
        py: "sm",
        pl: "sm",
        pr: "md",
        textStyle: "label12Regular",
        gap: "xs",
      },
    },
    {
      size: "large",
      content: "text",
      css: {
        textStyle: "label14Regular",
        py: "sm",
        px: "lg",
        height: "4xl",
      },
    },
    {
      size: "large",
      content: "icon",
      css: {
        p: "sm",
        maxW: "4xl",
      },
    },
    {
      size: "large",
      content: "iconText",
      css: {
        py: "sm",
        pl: "md",
        pr: "lg",
        textStyle: "label14Regular",
        gap: "sm",
      },
    },
  ],
});

export type ButtonVariantProps = RecipeVariantProps<typeof buttonStyles>;

export type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> &
  RecipeVariantProps<typeof buttonStyles> & {
    asChild?: boolean;
  };

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      className,
      variant = "filled",
      size = "medium",
      content = "text",
      asChild = false,
      disabled,
      ...props
    },
    ref,
  ) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        data-functional
        ref={ref}
        className={cx(buttonStyles({ variant, size, content }), className)}
        aria-disabled={disabled}
        disabled={disabled}
        {...props}
      />
    );
  },
);

Button.displayName = "Button";

export default Button;
