import { ArrowRight } from "lucide-react";
import type { ReactNode } from "react";
import { Link } from "react-router-dom";
import { AdminSectionLabel } from "../../components/AdminSectionLabel";

export type RosterRow = {
  /** Primary label, e.g. an org name or user email. */
  primary: string;
  /** Optional secondary line, e.g. a slug or org name. */
  secondary?: string;
  /** Optional trailing value (e.g. a member count). */
  trailing?: ReactNode;
  to: string;
};

/**
 * "Recently created" / "Recently active" condensed list. Avoids the
 * weight of a full table by using a two-line row pattern with monospace
 * accents on the secondary line.
 */
export const ResourceRoster = ({
  label,
  rows,
  emptyLabel,
  seeAllHref,
  isLoading
}: {
  label: string;
  rows: RosterRow[];
  emptyLabel: string;
  seeAllHref?: string;
  isLoading?: boolean;
}) => (
  <div className='space-y-3'>
    <AdminSectionLabel
      trailing={
        seeAllHref ? (
          <Link
            to={seeAllHref}
            className='inline-flex items-center gap-1 text-muted-foreground hover:text-foreground'
          >
            See all <ArrowRight className='size-3' />
          </Link>
        ) : undefined
      }
    >
      {label}
    </AdminSectionLabel>
    {isLoading ? (
      <div className='space-y-1.5'>
        {[0, 1, 2, 3].map((i) => (
          <div key={i} className='h-12 animate-pulse rounded-md bg-muted/40' />
        ))}
      </div>
    ) : rows.length === 0 ? (
      <p className='rounded-md border border-border/60 border-dashed bg-muted/20 px-4 py-6 text-center text-muted-foreground text-xs'>
        {emptyLabel}
      </p>
    ) : (
      <ul className='divide-y divide-border/60 overflow-hidden rounded-md border border-border/60 bg-card'>
        {rows.map((row, i) => (
          <li key={`${row.to}-${i}`}>
            <Link
              to={row.to}
              className='group flex items-center justify-between gap-3 px-3 py-2.5 transition-colors hover:bg-muted/40'
            >
              <div className='min-w-0 flex-1'>
                <div className='truncate font-medium text-sm'>{row.primary}</div>
                {row.secondary ? (
                  <div className='truncate font-mono text-[10px] text-muted-foreground'>
                    {row.secondary}
                  </div>
                ) : null}
              </div>
              {row.trailing ? (
                <span className='shrink-0 text-muted-foreground text-xs tabular-nums'>
                  {row.trailing}
                </span>
              ) : null}
              <ArrowRight className='size-3.5 shrink-0 text-muted-foreground/40 transition-colors group-hover:text-foreground' />
            </Link>
          </li>
        ))}
      </ul>
    )}
  </div>
);
