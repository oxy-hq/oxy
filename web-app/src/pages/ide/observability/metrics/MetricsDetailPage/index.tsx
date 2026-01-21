import { useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { ArrowLeft, Loader2 } from "lucide-react";
import useCurrentProject from "@/stores/useCurrentProject";
import ROUTES from "@/libs/utils/routes";
import { useMetricDetail } from "@/hooks/api/metrics";
import DetailHeader from "./components/DetailHeader";
import DetailStatsRow from "./components/DetailStatsRow";
import TrendChart from "./components/TrendChart";
import RelatedMetrics from "./components/RelatedMetrics";
import RecentUsageSection from "./components/RecentUsage/RecentUsageSection";
import type { DaysValue } from "./constants";

export default function MetricsDetailPage() {
  const { metricName } = useParams<{ metricName: string }>();
  const navigate = useNavigate();
  const { project } = useCurrentProject();
  const [daysFilter, setDaysFilter] = useState<DaysValue>(30);

  const decodedMetricName = metricName ? decodeURIComponent(metricName) : "";

  const {
    data: detailData,
    isLoading,
    error: detailError,
  } = useMetricDetail(decodedMetricName, daysFilter);

  const handleBack = () => {
    navigate(ROUTES.PROJECT(project?.id || "").IDE.OBSERVABILITY.METRICS);
  };

  const handleRelatedMetricClick = (relatedMetric: string) => {
    navigate(
      ROUTES.PROJECT(project?.id || "").IDE.OBSERVABILITY.METRIC(relatedMetric),
    );
  };

  const handleTraceClick = (traceId: string) => {
    navigate(
      ROUTES.PROJECT(project?.id || "").IDE.OBSERVABILITY.TRACE(traceId),
    );
  };

  if (isLoading) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center p-4 border-b">
          <Button
            variant="ghost"
            size="icon"
            onClick={handleBack}
            className="hover:bg-muted"
          >
            <ArrowLeft className="h-4 w-4" />
          </Button>
          <h1 className="text-xl font-semibold ml-4">{decodedMetricName}</h1>
        </div>
        <div className="flex-1 flex items-center justify-center">
          <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
        </div>
      </div>
    );
  }

  if (detailError || !detailData) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center p-4 border-b">
          <Button
            variant="ghost"
            size="icon"
            onClick={handleBack}
            className="hover:bg-muted"
          >
            <ArrowLeft className="h-4 w-4" />
          </Button>
          <h1 className="text-xl font-semibold ml-4">{decodedMetricName}</h1>
        </div>
        <div className="flex-1 flex items-center justify-center text-destructive">
          Failed to load metric details
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex flex-col min-h-0">
        <DetailHeader
          metricName={decodedMetricName}
          detailData={detailData}
          daysFilter={daysFilter}
          onBack={handleBack}
          onDaysFilterChange={setDaysFilter}
        />

        {/* Content */}
        <div className="p-6 flex-1 overflow-auto min-h-0 customScrollbar">
          <div className="max-w-6xl mx-auto space-y-6">
            <DetailStatsRow detailData={detailData} />

            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
              <TrendChart detailData={detailData} daysFilter={daysFilter} />
              <RelatedMetrics
                detailData={detailData}
                onMetricClick={handleRelatedMetricClick}
              />
            </div>

            <RecentUsageSection
              detailData={detailData}
              metricName={decodedMetricName}
              onTraceClick={handleTraceClick}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
