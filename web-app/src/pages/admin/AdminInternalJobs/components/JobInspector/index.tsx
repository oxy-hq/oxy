import { useState } from "react";
import { useQueueStats, useRecentFailures } from "@/hooks/api/internalJobs";
import { cn } from "@/libs/utils/cn";
import { useLive } from "../../LiveContext";
import { DeadLetterTab } from "./DeadLetterTab";
import { RecentFailuresTab } from "./RecentFailuresTab";

type TabId = "dead" | "failures";

/**
 * Tabbed job-state inspector. Replaces the two separate "Recent
 * failures" and "Dead-letter queue" sections with a single bordered
 * panel and a tabbed sub-nav. Each tab carries a live count badge so
 * the operator sees "Dead 12" at a glance even when they're on the
 * other tab.
 *
 * The dead-letter tab carries the bulk-action machinery (group by
 * shape, select, sticky retry/delete bar). The failures tab is
 * read-only — it's a chronological feed for triage, not a queue.
 */
export const JobInspector = () => {
  const { paused } = useLive();
  const [tab, setTab] = useState<TabId>("dead");

  // Derive the dead-count badge from queue stats (already polled by the
  // page header) instead of running a second 1-row peek poll against the
  // dead-letter endpoint. Saves a redundant request per tick and stays
  // honest — `queue_status='dead'` is the source of truth either way.
  const queueStats = useQueueStats({ paused });
  const failures = useRecentFailures(50, { paused });

  const deadCount = queueStats.data?.total.dead ?? null;
  const failureCount = failures.data?.length ?? null;

  return (
    <section className='space-y-4'>
      <header className='flex items-baseline justify-between'>
        <div>
          <h3 className='font-medium text-[10px] text-muted-foreground uppercase tracking-[0.14em]'>
            Job inspector
          </h3>
          <p className='mt-0.5 text-muted-foreground text-xs'>
            Group, multi-select, and triage failed and dead tasks.
          </p>
        </div>
      </header>

      <div className='border-border/60 border-b'>
        <nav className='-mb-px flex gap-1'>
          <TabButton
            active={tab === "dead"}
            label='Dead-letter'
            count={deadCount}
            tone='destructive'
            onClick={() => setTab("dead")}
          />
          <TabButton
            active={tab === "failures"}
            label='Recent failures'
            count={failureCount}
            tone='warn'
            onClick={() => setTab("failures")}
          />
        </nav>
      </div>

      <div>{tab === "dead" ? <DeadLetterTab /> : <RecentFailuresTab />}</div>
    </section>
  );
};

const TabButton = ({
  active,
  label,
  count,
  tone,
  onClick
}: {
  active: boolean;
  label: string;
  count: number | null;
  tone: "destructive" | "warn";
  onClick: () => void;
}) => (
  <button
    type='button'
    onClick={onClick}
    aria-current={active ? "page" : undefined}
    className={cn(
      "relative flex items-center gap-2 border-transparent border-b px-3 py-2.5 font-medium text-[11px] uppercase tracking-[0.14em] transition-colors",
      active
        ? "border-foreground text-foreground"
        : "text-muted-foreground hover:text-foreground/80"
    )}
  >
    <span>{label}</span>
    {count !== null && count > 0 ? (
      <span
        className={cn(
          "rounded-full px-1.5 py-0.5 font-medium text-[10px] tabular-nums tracking-normal",
          tone === "destructive"
            ? "bg-destructive/10 text-destructive"
            : "bg-amber-500/15 text-amber-700 dark:text-amber-400"
        )}
      >
        {count}
      </span>
    ) : null}
  </button>
);
