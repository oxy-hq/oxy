import { BarChart3, Cloud, List } from "lucide-react";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import PageHeader from "@/pages/ide/components/PageHeader";
import { DAYS_OPTIONS, type DaysValue, type ViewMode } from "../constants";

interface MetricsHeaderProps {
  viewMode: ViewMode;
  daysFilter: DaysValue;
  onViewModeChange: (mode: ViewMode) => void;
  onDaysFilterChange: (days: DaysValue) => void;
}

export default function MetricsHeader({
  viewMode,
  daysFilter,
  onViewModeChange,
  onDaysFilterChange
}: MetricsHeaderProps) {
  const actions = (
    <>
      <Tabs value={viewMode} onValueChange={(v) => onViewModeChange(v as ViewMode)}>
        <TabsList>
          <TabsTrigger value='grid'>
            <Cloud className='h-4 w-4' />
          </TabsTrigger>
          <TabsTrigger value='list'>
            <List className='h-4 w-4' />
          </TabsTrigger>
        </TabsList>
      </Tabs>

      <Tabs
        value={daysFilter.toString()}
        onValueChange={(v) => onDaysFilterChange(Number(v) as DaysValue)}
      >
        <TabsList>
          {DAYS_OPTIONS.map((option) => (
            <TabsTrigger key={option.value} value={option.value.toString()}>
              {option.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
    </>
  );

  return <PageHeader icon={BarChart3} title='Metric Analytics' actions={actions} />;
}
