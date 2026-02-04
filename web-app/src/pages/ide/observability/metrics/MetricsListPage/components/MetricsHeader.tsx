import { BarChart3, Cloud, List } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
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
  return (
    <div className='flex items-center justify-between border-b bg-background/95 p-4 backdrop-blur supports-[backdrop-filter]:bg-background/60'>
      <div className='flex items-center gap-3'>
        <div className='rounded-lg bg-primary/10 p-2'>
          <BarChart3 className='h-5 w-5 text-primary' />
        </div>
        <div>
          <h1 className='font-semibold text-xl'>Metric Analytics</h1>
          <p className='text-muted-foreground text-sm'>Track which metrics are queried most</p>
        </div>
      </div>
      <div className='flex items-center gap-3'>
        {/* View Mode Toggle */}
        <div className='flex gap-1 rounded-lg border bg-muted/30 p-1'>
          <Button
            variant={viewMode === "grid" ? "default" : "ghost"}
            size='sm'
            className='h-7 px-2'
            onClick={() => onViewModeChange("grid")}
          >
            <Cloud className='h-4 w-4' />
          </Button>
          <Button
            variant={viewMode === "list" ? "default" : "ghost"}
            size='sm'
            className='h-7 px-2'
            onClick={() => onViewModeChange("list")}
          >
            <List className='h-4 w-4' />
          </Button>
        </div>

        {/* Time Filter */}
        <div className='flex gap-1 rounded-lg border bg-muted/30 p-1'>
          {DAYS_OPTIONS.map((option) => (
            <Button
              key={option.value}
              variant={daysFilter === option.value ? "default" : "ghost"}
              size='sm'
              className='h-7 px-3'
              onClick={() => onDaysFilterChange(option.value)}
            >
              {option.label}
            </Button>
          ))}
        </div>
      </div>
    </div>
  );
}
