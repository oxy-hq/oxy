import type * as React from "react";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "../tooltip";

export type TooltipProps = React.ComponentProps<typeof TooltipContent> & {
  content: React.ReactNode;
  delayDuration?: number;
};

export const TooltipWrapper = ({
  children,
  tooltip,
  delayDuration = 300,
  ...tooltipContentProps
}: {
  children: React.ReactNode;
  tooltip?: string | TooltipProps;
  delayDuration?: number;
} & Omit<React.ComponentProps<typeof TooltipContent>, "content">) => {
  if (!tooltip) return <>{children}</>;

  // Convert string tooltip to object format
  const tooltipProps: TooltipProps = typeof tooltip === "string" ? { content: tooltip } : tooltip;

  const { content, delayDuration: duration = delayDuration } = tooltipProps;

  return (
    <TooltipProvider delayDuration={duration}>
      <Tooltip>
        <TooltipTrigger asChild>{children}</TooltipTrigger>
        <TooltipContent {...tooltipContentProps}>{content}</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
};
