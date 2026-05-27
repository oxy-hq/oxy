import { RefreshCw } from "lucide-react";
import type React from "react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import useCoordinatorLive from "@/hooks/api/coordinator/useCoordinatorLive";
import type { TimeRange } from "../components/constants";
import { type JobTypeChoice, JobTypeFilter, TimeRangePicker } from "../components/Filters";
import { ErrorState, LoadingState } from "../components/PageState";
import { AnomalyFeed } from "./components/AnomalyFeed";
import { HealthCards } from "./components/HealthCards";
import { Timeline } from "./components/Timeline";
import { useOverviewModel } from "./useOverviewModel";

/**
 * Overview — the landing page. Answers "is everything okay right now?" with
 * a health strip, the hero timeline, and a failure/anomaly feed. The header
 * filters (time range + job type) scope every surface below.
 */
const OverviewPage: React.FC = () => {
  const [range, setRange] = useState<TimeRange>("24h");
  const [typeFilter, setTypeFilter] = useState<JobTypeChoice>("all");
  const { runs, missingSlots, metrics, isPending, error, refetch } = useOverviewModel(
    range,
    typeFilter
  );

  // Real-time invalidation while the page is open.
  useCoordinatorLive();

  return (
    <div className='flex h-full flex-col'>
      <div className='flex flex-wrap items-center gap-3 border-border border-b px-4 py-2.5'>
        <TimeRangePicker value={range} onChange={setRange} />
        <JobTypeFilter value={typeFilter} onChange={setTypeFilter} />
        <Button
          variant='ghost'
          size='icon'
          onClick={refetch}
          className='ml-auto h-8 w-8'
          tooltip={{ content: "Refresh" }}
        >
          <RefreshCw className='h-4 w-4' />
        </Button>
      </div>

      {isPending ? (
        <LoadingState />
      ) : error ? (
        <ErrorState message='Failed to load coordinator activity' onRetry={refetch} />
      ) : (
        <div className='flex-1 overflow-y-auto'>
          <div className='flex flex-col gap-4 p-4'>
            <HealthCards metrics={metrics} />
            <Timeline runs={runs} missingSlots={missingSlots} range={range} />
            <AnomalyFeed runs={runs} />
          </div>
        </div>
      )}
    </div>
  );
};

export default OverviewPage;
