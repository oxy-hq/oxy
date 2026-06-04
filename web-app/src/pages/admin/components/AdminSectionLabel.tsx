import { cn } from "@/libs/shadcn/utils";

/**
 * Operator-console section divider. Uppercase, narrow tracking, tiny —
 * pitched as a "table of contents" stripe over dense data. Stays out of
 * the way visually while letting users scan vertically through a long
 * detail surface. Doesn't bring its own spacing; callers wrap it in the
 * grid that owns the column layout.
 */
export const AdminSectionLabel = ({
  children,
  className,
  trailing
}: {
  children: React.ReactNode;
  className?: string;
  /** Optional trailing element (e.g. a count or a small action button). */
  trailing?: React.ReactNode;
}) => (
  <div
    className={cn(
      "flex items-baseline justify-between border-border/60 border-b pb-2 font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]",
      className
    )}
  >
    <span>{children}</span>
    {trailing ? <span className='tracking-normal'>{trailing}</span> : null}
  </div>
);
