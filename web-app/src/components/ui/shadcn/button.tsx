import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { type VariantProps } from "class-variance-authority";

import { cn } from "@/libs/shadcn/utils";
import { buttonVariants } from "./utils/button-variants";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  TooltipProvider,
} from "./tooltip";

type TooltipConfig = {
  content: React.ReactNode;
  delayDuration?: number;
  sideOffset?: number;
} & Omit<React.ComponentProps<typeof TooltipContent>, "children">;

const Button = React.forwardRef<
  HTMLButtonElement,
  React.ButtonHTMLAttributes<HTMLButtonElement> &
    VariantProps<typeof buttonVariants> & {
      asChild?: boolean;
      tooltip?: string | TooltipConfig;
    }
>(({ className, variant, size, asChild = false, tooltip, ...props }, ref) => {
  const Comp = asChild ? Slot : "button";

  const buttonElement = (
    <Comp
      ref={ref}
      data-slot="button"
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  );

  if (!tooltip) {
    return buttonElement;
  }

  // Handle string tooltips and object tooltips
  const {
    content,
    delayDuration = 300,
    ...tooltipProps
  } = typeof tooltip === "string" ? { content: tooltip } : tooltip;

  return (
    <TooltipProvider delayDuration={delayDuration}>
      <Tooltip>
        <TooltipTrigger asChild>{buttonElement}</TooltipTrigger>
        <TooltipContent {...tooltipProps}>{content}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
});

Button.displayName = "Button";

export { Button };
