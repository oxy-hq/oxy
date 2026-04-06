import type * as React from "react";

import { cn } from "@/libs/shadcn/utils";

interface TextareaProps extends React.ComponentProps<"textarea"> {
  noFocusRing?: boolean;
}

function Textarea({ className, noFocusRing = false, ...props }: TextareaProps) {
  return (
    <textarea
      data-slot='textarea'
      className={cn(
        "field-sizing-content flex min-h-16 w-full rounded-md border border-input bg-input/30 px-3 py-2 text-base shadow-xs outline-none transition-[color,box-shadow] placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50 aria-invalid:border-destructive aria-invalid:ring-destructive/20 md:text-sm dark:aria-invalid:ring-destructive/40",
        !noFocusRing &&
          "focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50",
        className
      )}
      {...props}
    />
  );
}

export { Textarea };
