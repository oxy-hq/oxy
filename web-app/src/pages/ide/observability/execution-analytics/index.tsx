import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Activity } from "lucide-react";
import useCurrentProject from "@/stores/useCurrentProject";
import { timeRangeToDays } from "@/services/api/executionAnalytics";
import { useExecutionSummary } from "@/hooks/api/useExecutionAnalytics";
import PageHeader from "@/pages/ide/components/PageHeader";

import SummaryCards from "./components/SummaryCards";
import DistributionChart from "./components/DistributionChart";
import TrendChart from "./components/TrendChart";
import AgentBreakdownTable from "./components/AgentBreakdownTable";
import ExecutionList from "./components/ExecutionList";
import InfoLegend from "./components/InfoLegend";

import { ExecutionSummary } from "./types";

type TimeRange = "7d" | "30d" | "90d";

const TIME_RANGE_OPTIONS: { value: TimeRange; label: string }[] = [
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "90d", label: "90d" },
];

// Default empty state
const emptySummary: ExecutionSummary = {
  totalExecutions: 0,
  verifiedCount: 0,
  generatedCount: 0,
  verifiedPercent: 0,
  generatedPercent: 0,
  successRateVerified: 0,
  successRateGenerated: 0,
  mostExecutedType: "none",
  semanticQueryCount: 0,
  omniQueryCount: 0,
  sqlGeneratedCount: 0,
  workflowCount: 0,
  agentToolCount: 0,
};

export default function ExecutionAnalytics() {
  const { project } = useCurrentProject();
  const projectId = project?.id;

  const [timeRange, setTimeRange] = useState<TimeRange>("7d");
  const days = timeRangeToDays(timeRange);

  const {
    data: summary = emptySummary,
    isLoading,
    error,
    refetch,
  } = useExecutionSummary(projectId, { days });

  if (!projectId) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        No project selected
      </div>
    );
  }

  const timeRangeActions = (
    <div className="flex gap-1 border rounded-lg p-1 bg-muted/30">
      {TIME_RANGE_OPTIONS.map((option) => (
        <Button
          key={option.value}
          variant={timeRange === option.value ? "default" : "ghost"}
          size="sm"
          className="h-7 px-3"
          onClick={() => setTimeRange(option.value)}
        >
          {option.label}
        </Button>
      ))}
    </div>
  );

  return (
    <div className="flex flex-col h-full overflow-auto">
      <PageHeader
        icon={Activity}
        title="Execution Analytics"
        description="Track verified vs generated executions"
        actions={timeRangeActions}
      />

      <div className="p-6 flex-1 overflow-auto min-h-0 customScrollbar">
        {error ? (
          <div className="flex flex-col items-center justify-center h-64 text-muted-foreground">
            <p className="text-destructive">{error.message}</p>
            <Button
              variant="outline"
              size="sm"
              className="mt-4"
              onClick={() => refetch()}
            >
              Retry
            </Button>
          </div>
        ) : (
          <div className="max-w-7xl mx-auto space-y-6">
            <InfoLegend />

            <SummaryCards summary={summary} isLoading={isLoading} />

            <div className="grid md:grid-cols-2 gap-4">
              <DistributionChart summary={summary} isLoading={isLoading} />

              <TrendChart projectId={projectId} days={days} />
            </div>

            <AgentBreakdownTable projectId={projectId} days={days} limit={10} />
            <ExecutionList projectId={projectId} days={days} />
          </div>
        )}
      </div>
    </div>
  );
}
