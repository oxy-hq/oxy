import { useState, useCallback } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Sparkles, Loader2 } from "lucide-react";
import EmptyState from "@/components/ui/EmptyState";
import MetricCard from "./MetricCard";
import MetricListItem from "./MetricListItem";
import { useMetricsList } from "@/hooks/api/metrics";
import type { ViewMode, DaysValue } from "../../constants";
import { METRICS_PAGE_SIZE } from "../../constants";
import useCurrentProject from "@/stores/useCurrentProject";
import { useNavigate } from "react-router-dom";
import ROUTES from "@/libs/utils/routes";
import TablePagination from "@/components/ui/TablePagination";

interface MetricsGridProps {
  viewMode: ViewMode;
  daysFilter: DaysValue;
}

export default function MetricsGrid({
  viewMode,
  daysFilter,
}: MetricsGridProps) {
  const navigate = useNavigate();
  const { project } = useCurrentProject();
  const [currentPage, setCurrentPage] = useState(1);
  const offset = (currentPage - 1) * METRICS_PAGE_SIZE;

  // Fetch paginated metrics list
  const {
    data: listData,
    isLoading,
    isFetching,
  } = useMetricsList(daysFilter, METRICS_PAGE_SIZE, offset);

  const metrics = listData?.metrics || [];
  const maxCount = metrics[0]?.count || 1;
  const totalMetrics = listData?.total || 0;
  const totalPages = Math.ceil(totalMetrics / METRICS_PAGE_SIZE);

  const hasData = listData && listData?.total > 0;

  const handleMetricClick = useCallback(
    (metricName: string) => {
      navigate(
        ROUTES.PROJECT(project?.id || "").IDE.OBSERVABILITY.METRIC(metricName),
      );
    },
    [navigate, project?.id],
  );

  const renderContent = () => {
    if (isLoading || isFetching) {
      return (
        <div className="flex items-center justify-center py-12">
          <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
        </div>
      );
    }

    if (!hasData) {
      return (
        <div className="flex items-center justify-center py-12">
          <EmptyState
            title="No metrics data"
            description="No metric usage data available for the selected time range"
          />
        </div>
      );
    }

    if (viewMode === "grid") {
      return (
        <div className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-3 gap-4">
          {metrics.map((metric, index) => (
            <MetricCard
              key={metric.name}
              metric={metric}
              rank={offset + index + 1}
              maxCount={maxCount}
              onClick={() => handleMetricClick(metric.name)}
            />
          ))}
        </div>
      );
    }

    return (
      <div className="space-y-2">
        {metrics.map((metric, index) => (
          <MetricListItem
            key={metric.name}
            metric={metric}
            rank={offset + index + 1}
            maxCount={maxCount}
            onClick={() => handleMetricClick(metric.name)}
          />
        ))}
      </div>
    );
  };

  return (
    <Card>
      <CardHeader className="pb-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Sparkles className="h-5 w-5 text-primary" />
            <CardTitle>Top Metrics</CardTitle>
          </div>
          <span className="text-sm text-muted-foreground">
            {totalMetrics} metrics
          </span>
        </div>
        <CardDescription>
          Click any metric to explore detailed usage patterns
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {renderContent()}

        {/* Pagination Controls */}
        {!isLoading && !isFetching && (
          <TablePagination
            currentPage={currentPage}
            totalPages={totalPages}
            totalItems={totalMetrics}
            pageSize={METRICS_PAGE_SIZE}
            onPageChange={setCurrentPage}
            itemLabel="metrics"
          />
        )}
      </CardContent>
    </Card>
  );
}
