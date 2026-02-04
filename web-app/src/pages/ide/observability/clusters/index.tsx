import { Loader2 } from "lucide-react";
import { useMemo, useState } from "react";
import EmptyState from "@/components/ui/EmptyState";
import { Button } from "@/components/ui/shadcn/button";
import useClusterMap from "@/hooks/api/traces/useClusterMap";
import ClusterBreakdownTable from "./components/ClusterBreakdownTable";
import ClusterDistributionChart from "./components/ClusterDistributionChart";
import ClustersHeader from "./components/ClustersHeader";
import ClusterVisualization from "./components/ClusterVisualization";
import Sidebar from "./components/Sidebar";
import SummaryCards from "./components/SummaryCards";
import { type TimeRange, timeRangeToDays } from "./types";

export default function ClustersV2Page() {
  const [timeRange, setTimeRange] = useState<TimeRange>("30d");
  const [limit, setLimit] = useState(100);
  const [source, setSource] = useState<string | undefined>(undefined);

  const days = timeRangeToDays(timeRange);
  const { data, isLoading, error, refetch } = useClusterMap(limit, days, true, source);

  const points = useMemo(() => data?.points ?? [], [data?.points]);
  const clusters = useMemo(() => data?.clusters ?? [], [data?.clusters]);

  const clusterColorMap = useMemo(() => {
    const map = new Map<number, string>();
    clusters.forEach((c) => {
      map.set(c.clusterId, c.color);
    });
    return map;
  }, [clusters]);

  if (error) {
    return (
      <div className='flex h-full flex-col items-center justify-center text-muted-foreground'>
        <p className='mb-4 text-destructive'>Failed to load cluster data: {error.message}</p>
        <Button variant='outline' size='sm' onClick={() => refetch()}>
          Retry
        </Button>
      </div>
    );
  }

  const hasData = points.length > 0;

  return (
    <div className='flex h-full flex-col'>
      <ClustersHeader
        timeRange={timeRange}
        limit={limit}
        source={source}
        onTimeRangeChange={setTimeRange}
        onLimitChange={setLimit}
        onSourceChange={setSource}
      />

      <div className='flex flex-1 overflow-hidden'>
        {/* Main Content */}
        <div className='customScrollbar min-h-0 flex-1 overflow-auto p-6'>
          {isLoading && !hasData && (
            <div className='flex h-64 items-center justify-center'>
              <Loader2 className='h-8 w-8 animate-spin text-muted-foreground' />
            </div>
          )}
          {!isLoading && !hasData && (
            <div className='flex h-64 items-center justify-center'>
              <EmptyState
                title='No traces found'
                description='No trace data available for the selected time range and filters'
              />
            </div>
          )}
          {hasData && (
            <div className='mx-auto max-w-7xl space-y-6'>
              {/* Summary Stats */}
              <SummaryCards clusters={clusters} points={points} isLoading={isLoading} />

              {/* Cluster Distribution + Top Clusters Row */}
              <div className='grid grid-cols-1 gap-6 lg:grid-cols-2'>
                <ClusterDistributionChart clusters={clusters} isLoading={isLoading} />
                <Sidebar clusters={clusters} points={points} isLoading={isLoading} />
              </div>

              {/* Cluster Visualization (List + Chart/Grid with toggle) */}
              <ClusterVisualization
                clusters={clusters}
                points={points}
                clusterColorMap={clusterColorMap}
              />

              {/* Cluster Breakdown Table */}
              <ClusterBreakdownTable clusters={clusters} points={points} isLoading={isLoading} />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
