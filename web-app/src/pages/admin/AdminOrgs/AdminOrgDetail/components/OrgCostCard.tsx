import { DollarSign } from "lucide-react";
import { CostBars } from "@/pages/admin/AdminTenants/components/LlmCostSection/CostBars";
import type { OrgUsageDetail } from "@/services/api/adminMetrics";
import { AdminSectionLabel } from "../../../components/AdminSectionLabel";
import { compactInt, usd } from "../format";

/**
 * Overview cost summary + daily trend for one org. Backed by the org-scoped
 * `/admin/metrics/orgs/{id}/llm-usage` (correct for any tenant, not just the
 * top-10 leaderboard), with the daily series rendered by the shared `CostBars`.
 */
export const OrgCostCard = ({
  usage,
  days,
  isLoading
}: {
  usage: OrgUsageDetail | null | undefined;
  days: number;
  isLoading: boolean;
}) => (
  <section className='space-y-3'>
    <AdminSectionLabel>LLM cost · last {days}d</AdminSectionLabel>
    <div className='rounded-lg border border-border/60 bg-card p-5'>
      {isLoading ? (
        <div className='h-28 animate-pulse rounded bg-muted/40' />
      ) : !usage || usage.total.run_count === 0 ? (
        <p className='text-muted-foreground text-xs'>No LLM activity in this window.</p>
      ) : (
        <div className='space-y-4'>
          <div className='flex items-baseline gap-2'>
            <DollarSign className='size-4 text-muted-foreground' />
            <span className='font-semibold text-2xl tabular-nums tracking-tight'>
              {usd(usage.total.cost_usd)}
            </span>
          </div>
          <CostBars data={usage.by_day} />
          <dl className='grid grid-cols-3 gap-2 text-xs'>
            <div className='space-y-0.5'>
              <dt className='text-muted-foreground'>Runs</dt>
              <dd className='font-medium tabular-nums'>{compactInt(usage.total.run_count)}</dd>
            </div>
            <div className='space-y-0.5'>
              <dt className='text-muted-foreground'>Input tok</dt>
              <dd className='font-medium tabular-nums'>{compactInt(usage.total.input_tokens)}</dd>
            </div>
            <div className='space-y-0.5'>
              <dt className='text-muted-foreground'>Output tok</dt>
              <dd className='font-medium tabular-nums'>{compactInt(usage.total.output_tokens)}</dd>
            </div>
          </dl>
        </div>
      )}
    </div>
  </section>
);
