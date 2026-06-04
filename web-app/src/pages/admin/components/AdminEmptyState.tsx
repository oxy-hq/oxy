import type { ComponentType } from "react";
import { cn } from "@/libs/shadcn/utils";

/**
 * Bordered empty state with icon, headline, and a soft suggestion line.
 * Used as a tenant-list placeholder when filters or searches return
 * nothing, or when a resource hasn't been seeded yet. Looks intentional
 * — no animated spinner, no "nothing to see here" copy.
 */
export const AdminEmptyState = ({
  icon: Icon,
  title,
  description,
  action,
  className
}: {
  icon: ComponentType<{ className?: string }>;
  title: string;
  description?: string;
  action?: React.ReactNode;
  className?: string;
}) => (
  <div
    className={cn(
      "flex flex-col items-center justify-center gap-3 rounded-lg border border-border/60 border-dashed bg-muted/20 px-6 py-12 text-center",
      className
    )}
  >
    <div className='rounded-full border border-border/60 bg-background p-2 text-muted-foreground'>
      <Icon className='size-5' />
    </div>
    <div className='space-y-1'>
      <p className='font-medium text-sm'>{title}</p>
      {description ? (
        <p className='mx-auto max-w-md text-muted-foreground text-xs'>{description}</p>
      ) : null}
    </div>
    {action ? <div className='pt-1'>{action}</div> : null}
  </div>
);
