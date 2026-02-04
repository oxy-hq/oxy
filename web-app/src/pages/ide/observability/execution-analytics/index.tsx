import { Activity } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { useExecutionSummary } from "@/hooks/api/useExecutionAnalytics";
import PageHeader from "@/pages/ide/components/PageHeader";
import { timeRangeToDays } from "@/services/api/executionAnalytics";
import useCurrentProject from "@/stores/useCurrentProject";
import AgentBreakdownTable from "./components/AgentBreakdownTable";
import DistributionChart from "./components/DistributionChart";
import ExecutionList from "./components/ExecutionList";
import InfoLegend from "./components/InfoLegend";
import SummaryCards from "./components/SummaryCards";
import TrendChart from "./components/TrendChart";

import type { ExecutionSummary } from "./types";

type TimeRange = "7d" | "30d" | "90d";

const TIME_RANGE_OPTIONS: { value: TimeRange; label: string }[] = [
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "90d", label: "90d" }
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
  agentToolCount: 0
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
    refetch
  } = useExecutionSummary(projectId, { days });

  if (!projectId) {
    return (
      <div className='flex h-full items-center justify-center text-muted-foreground'>
        No project selected
      </div>
    );
  }

  const timeRangeActions = (
    <div className='flex gap-1 rounded-lg border bg-muted/30 p-1'>
      {TIME_RANGE_OPTIONS.map((option) => (
        <Button
          key={option.value}
          variant={timeRange === option.value ? "default" : "ghost"}
          size='sm'
          className='h-7 px-3'
          onClick={() => setTimeRange(option.value)}
        >
          {option.label}
        </Button>
      ))}
    </div>
  );

  return (
    <div className='flex h-full flex-col overflow-auto'>
      <PageHeader
        icon={Activity}
        title='Execution Analytics'
        description='Track verified vs generated executions'
        actions={timeRangeActions}
      />

      <div className='customScrollbar min-h-0 flex-1 overflow-auto p-6'>
        {error ? (
          <div className='flex h-64 flex-col items-center justify-center text-muted-foreground'>
            <p className='text-destructive'>{error.message}</p>
            <Button variant='outline' size='sm' className='mt-4' onClick={() => refetch()}>
              Retry
            </Button>
          </div>
        ) : (
          <div className='mx-auto max-w-7xl space-y-6'>
            <InfoLegend />

            <SummaryCards summary={summary} isLoading={isLoading} />

            <div className='grid gap-4 md:grid-cols-2'>
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
