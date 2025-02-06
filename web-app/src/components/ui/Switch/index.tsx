"use client";

import * as React from "react";

import * as SwitchPrimitives from "@radix-ui/react-switch";
import { css, cx } from "styled-system/css";

const rootStyles = css({
  w: "36px",
  h: "20px",
  borderRadius: "full",
  bgColor: "surface.secondary",
  // border
  shadow: "0 0 0 1px token(colors.border.primary)",
  transition: "all 200ms",
  cursor: "pointer",
  "&[data-state=checked]": {
    bgColor: "surface.contrast",
    // border
    shadow: "0 0 0 1px token(colors.surface.contrast)",
  },
  _disabled: {
    bgColor: "surface.secondary",
    // border
    shadow: "0 0 0 1px token(colors.surface.secondary)",
    cursor: "not-allowed",
  },
});

const thumbStyles = css({
  display: "block",
  width: "16px",
  height: "16px",
  borderRadius: "full",
  transition: "all 200ms",
  transform: "translateX(1px)",
  willChange: "transform",
  bgColor: "text.less-contrast",
  pointerEvents: "none",
  "&[data-state=checked]": {
    transform: "translateX(17px)",
    bgColor: "text.contrast",
  },
  _disabled: {
    bgColor: "surface.tertiary",
  },
});

const Switch = React.forwardRef<
  React.ElementRef<typeof SwitchPrimitives.Root>,
  React.ComponentPropsWithoutRef<typeof SwitchPrimitives.Root>
>(({ className, ...props }, ref) => (
  <SwitchPrimitives.Root
    className={cx(rootStyles, className, "peer")}
    {...props}
    ref={ref}
  >
    <SwitchPrimitives.Thumb className={cx(thumbStyles)} />
  </SwitchPrimitives.Root>
));
Switch.displayName = SwitchPrimitives.Root.displayName;

export { Switch };
