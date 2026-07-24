import type { ReactNode } from "react";
import { cn } from "../cn";

// The one surface primitive: a flat, sharp-cornered card with a hairline
// border, an uppercase caption header, and a body. Every panel and tile in the
// app is built from this.
export function Panel({
  title,
  right,
  children,
  className
}: {
  title: ReactNode;
  right?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      className={cn(
        // `min-w-0` keeps the card shrinkable in a flex row (so child
        // `truncate` still works) without clipping vertical content.
        "flex min-w-0 flex-col gap-2 border border-border bg-card p-3 text-muted-foreground",
        className
      )}
    >
      <div className='flex items-center justify-between border-border border-b pb-1.5 text-[10px] text-muted-foreground uppercase tracking-wider'>
        <span>{title}</span>
        {right ?? null}
      </div>
      <div className='flex min-h-0 flex-1 flex-col gap-2 text-xs'>{children}</div>
    </div>
  );
}
