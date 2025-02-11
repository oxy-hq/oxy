"use client";

import * as React from "react";

import type { RecipeVariantProps } from "styled-system/css";

import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { css, cx, sva } from "styled-system/css";
import { hstack } from "styled-system/patterns";

/**
 * TooltipProvider
 * @see https://radix-ui.com/primitives/docs/components/tooltip#provider
 */
const TooltipProvider = TooltipPrimitive.Provider;

/**
 * Tooltip
 * @see https://radix-ui.com/primitives/docs/components/tooltip#root
 */
const TooltipRoot = TooltipPrimitive.Root;

/**
 * TooltipTrigger
 * @see https://radix-ui.com/primitives/docs/components/tooltip#root
 */
const TooltipTrigger = TooltipPrimitive.Trigger;

/**
 * TooltipPortal
 * @see https://radix-ui.com/primitives/docs/components/tooltip#portal
 */
const TooltipPortal = TooltipPrimitive.Portal;

const contentStyles = css({
  backgroundColor: "surface.contrast",
  color: "text.contrast",
  py: "xs",
  px: "sm",
  borderRadius: "minimal",
  boxShadow: "primary",
  textStyle: "label12Regular",
  "&[data-state='instant-open']": {
    animation: "fadeIn 200ms ease-out",
  },
  "&[data-state='delayed-open']": {
    animation: "fadeIn 200ms ease-out",
  },
  "&[data-state='closed']": {
    animation: "fadeOut 200ms ease-in",
  },
});

/**
 * TooltipContent
 * @see https://radix-ui.com/primitives/docs/components/tooltip#content
 */
const TooltipContent = React.forwardRef<
  React.ElementRef<typeof TooltipPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TooltipPrimitive.Content>
>(({ className, sideOffset = 4, ...props }, ref) => (
  <TooltipPrimitive.Content
    ref={ref}
    sideOffset={sideOffset}
    className={cx(contentStyles, className)}
    {...props}
  />
));
TooltipContent.displayName = TooltipPrimitive.Content.displayName;

type TooltipProps = React.ComponentPropsWithoutRef<typeof TooltipRoot> &
  RecipeVariantProps<typeof tooltipStyles> & {
    content: React.ReactNode;
    isPortal?: boolean;
    contentProps?: React.ComponentPropsWithoutRef<typeof TooltipContent>;
  };

const tooltipStyles = sva({
  slots: ["content", "shortcut"],
  base: {
    content: hstack.raw({
      gap: "xs",
    }),
    shortcut: {
      color: "text.secondary",
    },
  },
  variants: {
    variant: {
      default: {},
      multiline: {
        content: {
          textStyle: "paragraph12Regular",
          maxWidth: "180px",
        },
      },
    },
  },
});

function Tooltip({
  children,
  content,
  contentProps,
  variant = "default",
  ...props
}: TooltipProps) {
  const classes = tooltipStyles({ variant });
  const Container = props.isPortal ? TooltipPortal : React.Fragment;

  return (
    <TooltipRoot {...props}>
      <Container>
        <TooltipTrigger asChild>{children}</TooltipTrigger>
        <TooltipContent {...contentProps}>
          <div className={classes.content}>{content}</div>
        </TooltipContent>
      </Container>
    </TooltipRoot>
  );
}

export {
  Tooltip,
  TooltipRoot,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
  TooltipPortal,
};
