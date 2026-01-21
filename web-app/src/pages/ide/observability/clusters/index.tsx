import { useState, useMemo } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Loader2 } from "lucide-react";
import useClusterMap from "@/hooks/api/traces/useClusterMap";
import EmptyState from "@/components/ui/EmptyState";

import { type TimeRange, timeRangeToDays } from "./types";
import ClustersHeader from "./components/ClustersHeader";
import SummaryCards from "./components/SummaryCards";
import ClusterDistributionChart from "./components/ClusterDistributionChart";
import Sidebar from "./components/Sidebar";
import ClusterVisualization from "./components/ClusterVisualization";
import ClusterBreakdownTable from "./components/ClusterBreakdownTable";

export default function ClustersV2Page() {
  const [timeRange, setTimeRange] = useState<TimeRange>("30d");
  const [limit, setLimit] = useState(100);
  const [source, setSource] = useState<string | undefined>(undefined);

  const days = timeRangeToDays(timeRange);
  const { data, isLoading, error, refetch } = useClusterMap(
    limit,
    days,
    true,
    source,
  );

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
      <div className="flex flex-col items-center justify-center h-full text-muted-foreground">
        <p className="text-destructive mb-4">
          Failed to load cluster data: {error.message}
        </p>
        <Button variant="outline" size="sm" onClick={() => refetch()}>
          Retry
        </Button>
      </div>
    );
  }

  const hasData = points.length > 0;

  return (
    <div className="flex flex-col h-full">
      <ClustersHeader
        timeRange={timeRange}
        limit={limit}
        source={source}
        onTimeRangeChange={setTimeRange}
        onLimitChange={setLimit}
        onSourceChange={setSource}
      />

      <div className="flex-1 flex overflow-hidden">
        {/* Main Content */}
        <div className="flex-1 p-6 overflow-auto min-h-0 customScrollbar">
          {isLoading && !hasData && (
            <div className="flex items-center justify-center h-64">
              <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
            </div>
          )}
          {!isLoading && !hasData && (
            <div className="flex items-center justify-center h-64">
              <EmptyState
                title="No traces found"
                description="No trace data available for the selected time range and filters"
              />
            </div>
          )}
          {hasData && (
            <div className="max-w-7xl mx-auto space-y-6">
              {/* Summary Stats */}
              <SummaryCards
                clusters={clusters}
                points={points}
                isLoading={isLoading}
              />

              {/* Cluster Distribution + Top Clusters Row */}
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                <ClusterDistributionChart
                  clusters={clusters}
                  isLoading={isLoading}
                />
                <Sidebar
                  clusters={clusters}
                  points={points}
                  isLoading={isLoading}
                />
              </div>

              {/* Cluster Visualization (List + Chart/Grid with toggle) */}
              <ClusterVisualization
                clusters={clusters}
                points={points}
                clusterColorMap={clusterColorMap}
              />

              {/* Cluster Breakdown Table */}
              <ClusterBreakdownTable
                clusters={clusters}
                points={points}
                isLoading={isLoading}
              />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
