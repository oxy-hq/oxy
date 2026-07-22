import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import * as React from "react";
import { ShellPortalContext } from "./portal";

/**
 * Hover tooltip for shell controls. Mirrors the web-app's Radix tooltip
 * (zero delay, side arrow, `--zinc-200`/muted surface) but portals into the
 * shell scope so the namespaced tokens and dark-mode class reach it.
 */
export function ShellTooltip({
  content,
  side = "right",
  children
}: {
  content: React.ReactNode;
  side?: "top" | "right" | "bottom" | "left";
  children: React.ReactNode;
}) {
  const container = React.useContext(ShellPortalContext);
  return (
    <TooltipPrimitive.Provider delayDuration={0}>
      <TooltipPrimitive.Root>
        <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
        <TooltipPrimitive.Portal container={container ?? undefined}>
          <TooltipPrimitive.Content
            side={side}
            sideOffset={0}
            className='oxy-shell-scope oxy-shell-tooltip'
          >
            {content}
            <TooltipPrimitive.Arrow className='oxy-shell-tooltip__arrow' />
          </TooltipPrimitive.Content>
        </TooltipPrimitive.Portal>
      </TooltipPrimitive.Root>
    </TooltipPrimitive.Provider>
  );
}
