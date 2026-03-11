import { Activity, ArrowLeft, Hash } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
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

  const actions = (
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
  );

  return (
    <div className='flex items-center border-b bg-background/95 px-4 py-1 backdrop-blur supports-[backdrop-filter]:bg-background/60'>
      <Button variant='ghost' size='icon' onClick={onBack} className='mr-1 h-7 w-7 hover:bg-muted'>
        <ArrowLeft className='h-4 w-4' />
      </Button>
      <div className='flex flex-1 items-center justify-between'>
        <div className='flex min-h-8 items-center gap-3'>
          <Hash className='h-4 w-4 text-primary' />
          <h1 className='font-semibold text-sm'>{metricName}</h1>
          <span className='flex items-center gap-1 text-muted-foreground text-xs'>
            <Activity className='h-3 w-3' />
            {totalQueries.toLocaleString()} queries
          </span>
        </div>
        <div className='flex items-center gap-3'>{actions}</div>
      </div>
    </div>
  );
}
