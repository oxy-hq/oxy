import { useState } from "react";
import { useMetricsAnalytics } from "@/hooks/api/metrics";
import MetricsGrid from "./components/MetricsGrid/MetricsGrid";
import MetricsHeader from "./components/MetricsHeader";
import Sidebar from "./components/Sidebar/Sidebar";
import StatsRow from "./components/StatsRow/StatsRow";
import type { DaysValue, ViewMode } from "./constants";

export default function MetricsListPage() {
  const [daysFilter, setDaysFilter] = useState<DaysValue>(7);
  const [viewMode, setViewMode] = useState<ViewMode>("grid");

  // Fetch analytics summary
  const {
    data: analyticsData,
    isLoading: isAnalyticsLoading,
    error
  } = useMetricsAnalytics(daysFilter);

  if (error) {
    return (
      <div className='flex h-full items-center justify-center text-destructive'>
        Failed to load metrics data: {error.message}
      </div>
    );
  }

  return (
    <div className='flex h-full flex-col'>
      <div className='flex min-h-0 flex-1 flex-col'>
        <MetricsHeader
          viewMode={viewMode}
          daysFilter={daysFilter}
          onViewModeChange={setViewMode}
          onDaysFilterChange={setDaysFilter}
        />

        {/* Content */}
        <div className='customScrollbar min-h-0 flex-1 overflow-auto p-6'>
          <div className='mx-auto max-w-7xl space-y-6'>
            <StatsRow
              analyticsData={analyticsData}
              daysFilter={daysFilter}
              isLoading={isAnalyticsLoading}
            />

            {/* Main Content Grid */}
            <div className='grid grid-cols-1 gap-6 lg:grid-cols-3'>
              <div className='lg:col-span-2'>
                <MetricsGrid key={daysFilter} viewMode={viewMode} daysFilter={daysFilter} />
              </div>

              {/* Sidebar - Takes 1 column */}
              <Sidebar analyticsData={analyticsData} isLoading={isAnalyticsLoading} />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
