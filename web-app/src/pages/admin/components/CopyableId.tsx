import { Check, Copy } from "lucide-react";
import { type MouseEvent, useState } from "react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/shadcn/tooltip";
import { cn } from "@/libs/shadcn/utils";

/**
 * Monospace, truncated identifier that copies its FULL value to the
 * clipboard on click. The on-the-wire value is never lost: it's exposed
 * via the native `title`, a Tooltip, and the clipboard write. A brief
 * check icon confirms the copy without a toast.
 *
 * Shared across admin surfaces for the long UUID/SHA values operators
 * routinely need to paste — `org_id`, `workspace_id`, `revision_id`,
 * `git_sha`.
 */
export const CopyableId = ({
  value,
  /** Chars to show before the ellipsis. SHAs read fine at 12; UUIDs at 8. */
  head = 8,
  /** When true, render the full value instead of truncating. */
  full = false,
  className
}: {
  value: string | null | undefined;
  head?: number;
  full?: boolean;
  className?: string;
}) => {
  const [copied, setCopied] = useState(false);

  if (typeof value !== "string" || value.length === 0) {
    return <span className='font-mono text-muted-foreground text-xs'>—</span>;
  }

  const display = full || value.length <= head ? value : `${value.slice(0, head)}…`;

  // Stop propagation so copying an id inside a click-navigable row/card
  // never also triggers the parent's onClick (navigation).
  const onCopy = async (e: MouseEvent) => {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch (err) {
      console.error("Failed to copy id to clipboard", err);
    }
  };

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type='button'
          onClick={onCopy}
          title={value}
          aria-label={`Copy ${value}`}
          className={cn(
            "group inline-flex max-w-full items-center gap-1 rounded-sm px-1 py-0.5 font-mono text-xs",
            "text-foreground/90 transition-colors hover:bg-muted hover:text-foreground",
            "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
            className
          )}
        >
          <span className='truncate tabular-nums'>{display}</span>
          {copied ? (
            <Check className='size-3 shrink-0 text-primary' aria-hidden />
          ) : (
            <Copy
              className='size-3 shrink-0 text-muted-foreground/0 transition-colors group-hover:text-muted-foreground'
              aria-hidden
            />
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent side='top' className='font-mono text-xs'>
        {copied ? "Copied" : value}
      </TooltipContent>
    </Tooltip>
  );
};
