import { useState } from "react";
import { useMetricsAnalytics } from "@/hooks/api/metrics";
import MetricsHeader from "./components/MetricsHeader";
import StatsRow from "./components/StatsRow/StatsRow";
import MetricsGrid from "./components/MetricsGrid/MetricsGrid";
import Sidebar from "./components/Sidebar/Sidebar";
import { type DaysValue, type ViewMode } from "./constants";

export default function MetricsListPage() {
  const [daysFilter, setDaysFilter] = useState<DaysValue>(7);
  const [viewMode, setViewMode] = useState<ViewMode>("grid");

  // Fetch analytics summary
  const {
    data: analyticsData,
    isLoading: isAnalyticsLoading,
    error,
  } = useMetricsAnalytics(daysFilter);

  if (error) {
    return (
      <div className="flex items-center justify-center h-full text-destructive">
        Failed to load metrics data: {error.message}
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex flex-col min-h-0">
        <MetricsHeader
          viewMode={viewMode}
          daysFilter={daysFilter}
          onViewModeChange={setViewMode}
          onDaysFilterChange={setDaysFilter}
        />

        {/* Content */}
        <div className="p-6 flex-1 overflow-auto min-h-0 customScrollbar">
          <div className="max-w-7xl mx-auto space-y-6">
            <StatsRow
              analyticsData={analyticsData}
              daysFilter={daysFilter}
              isLoading={isAnalyticsLoading}
            />

            {/* Main Content Grid */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
              <div className="lg:col-span-2">
                <MetricsGrid
                  key={daysFilter}
                  viewMode={viewMode}
                  daysFilter={daysFilter}
                />
              </div>

              {/* Sidebar - Takes 1 column */}
              <Sidebar
                analyticsData={analyticsData}
                isLoading={isAnalyticsLoading}
              />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
