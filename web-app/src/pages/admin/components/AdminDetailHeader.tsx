import type { ComponentType, ReactNode } from "react";
import { Link } from "react-router-dom";
import { cn } from "@/libs/shadcn/utils";

/**
 * Page-level header for an Admin detail surface (org / user / workspace).
 * The eyebrow doubles as a breadcrumb. The "identity row" (large avatar +
 * primary name + secondary metadata + actions) is the visual anchor that
 * tells the operator "you are inspecting THIS entity" — large enough to
 * never feel like a list-cell expansion.
 */
export const AdminDetailHeader = ({
  eyebrow,
  icon: Icon,
  title,
  subtitle,
  status,
  actions
}: {
  eyebrow: ReactNode;
  icon: ComponentType<{ className?: string }>;
  title: string;
  /** Secondary identifier (slug, email, parent org link). Renders in mono. */
  subtitle?: ReactNode;
  /** Status pill, badges, etc. — rendered to the right of the title row. */
  status?: ReactNode;
  /** Primary destructive / mutating actions. Rendered on the right. */
  actions?: ReactNode;
}) => (
  <header className='space-y-5'>
    <p className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
      {eyebrow}
    </p>
    <div className='flex flex-col gap-5 sm:flex-row sm:items-start sm:justify-between'>
      <div className='flex min-w-0 items-start gap-4'>
        <div
          className={cn(
            "flex size-14 shrink-0 items-center justify-center rounded-xl",
            "border border-border/60 bg-card text-foreground/70 shadow-[0_1px_0_0_color-mix(in_srgb,var(--border)_60%,transparent)]"
          )}
        >
          <Icon className='size-6' />
        </div>
        <div className='flex min-w-0 flex-col gap-1.5'>
          <div className='flex flex-wrap items-center gap-3'>
            <h1 className='truncate font-semibold text-3xl tracking-tight'>{title}</h1>
            {status}
          </div>
          {subtitle ? (
            <div className='flex flex-wrap items-center gap-x-3 gap-y-1 text-muted-foreground text-sm'>
              {subtitle}
            </div>
          ) : null}
        </div>
      </div>
      {actions ? <div className='flex shrink-0 flex-wrap items-center gap-2'>{actions}</div> : null}
    </div>
  </header>
);

/**
 * Convenience renderer for the eyebrow breadcrumb. Each segment is a link
 * (or plain text for the current segment). Separated by middle dot.
 */
export const AdminDetailEyebrow = ({
  segments
}: {
  segments: Array<{ label: string; to?: string }>;
}) => (
  <>
    {segments.map((seg, i) => (
      <span key={`${seg.label}-${i}`}>
        {i > 0 ? <span className='mx-1.5 text-muted-foreground/50'>·</span> : null}
        {seg.to ? (
          <Link to={seg.to} className='hover:text-foreground'>
            {seg.label}
          </Link>
        ) : (
          <span className='text-foreground'>{seg.label}</span>
        )}
      </span>
    ))}
  </>
);
