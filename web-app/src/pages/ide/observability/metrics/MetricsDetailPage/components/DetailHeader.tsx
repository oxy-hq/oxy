import { Activity, ArrowLeft, Hash } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import type { MetricDetailResponse } from "@/services/api/metrics";
import { DAYS_OPTIONS, type DaysValue } from "../constants";

interface DetailHeaderProps {
  metricName: string;
  detailData: MetricDetailResponse;
  daysFilter: DaysValue;
  onBack: () => void;
  onDaysFilterChange: (days: DaysValue) => void;
}

export default function DetailHeader({
  metricName,
  detailData,
  daysFilter,
  onBack,
  onDaysFilterChange
}: DetailHeaderProps) {
  const totalQueries = detailData.total_queries;
  return (
    <div className='flex items-center justify-between border-b bg-background/95 p-4 backdrop-blur supports-[backdrop-filter]:bg-background/60'>
      <div className='flex items-center gap-4'>
        <Button variant='ghost' size='icon' onClick={onBack} className='hover:bg-muted'>
          <ArrowLeft className='h-4 w-4' />
        </Button>
        <div>
          <div className='flex items-center gap-2'>
            <Hash className='h-5 w-5 text-primary' />
            <h1 className='font-semibold text-xl'>{metricName}</h1>
          </div>
          <p className='mt-0.5 flex items-center gap-2 text-muted-foreground text-sm'>
            <Activity className='h-3 w-3' />
            {totalQueries.toLocaleString()} total queries
          </p>
        </div>
      </div>
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
  );
}
