import {
  BarChart3,
  LucideBot,
  LucideWorkflow,
  Minus,
  TrendingDown,
  TrendingUp
} from "lucide-react";
import { Card, CardContent } from "@/components/ui/shadcn/card";
import { cn } from "@/libs/shadcn/utils";
import type { MetricDetailResponse } from "@/services/api/metrics";

function TrendIcon({ isPositive, isNegative }: { isPositive?: boolean; isNegative?: boolean }) {
  if (isPositive) {
    return (
      <div className='rounded-lg bg-green-500/10 p-2'>
        <TrendingUp className='h-5 w-5 text-green-400' />
      </div>
    );
  }
  if (isNegative) {
    return (
      <div className='rounded-lg bg-red-500/10 p-2'>
        <TrendingDown className='h-5 w-5 text-red-400' />
      </div>
    );
  }
  return (
    <div className='rounded-lg bg-muted p-2'>
      <Minus className='h-5 w-5 text-muted-foreground' />
    </div>
  );
}

interface DetailStatsRowProps {
  detailData: MetricDetailResponse;
}

export default function DetailStatsRow({ detailData }: DetailStatsRowProps) {
  const totalQueries = detailData.total_queries;
  const trendVsLastPeriod = detailData.trend_vs_last_period;
  const viaAgent = detailData.via_agent;
  const viaWorkflow = detailData.via_workflow;

  const trend = trendVsLastPeriod;
  const isPositive = trend?.startsWith("+");
  const isNegative = trend?.startsWith("-");

  return (
    <div className='grid grid-cols-1 gap-4 md:grid-cols-4'>
      <Card className='overflow-hidden'>
        <CardContent className='p-4'>
          <div className='flex items-center gap-3'>
            <div className='rounded-lg bg-primary/10 p-2'>
              <BarChart3 className='h-5 w-5 text-primary' />
            </div>
            <div>
              <p className='font-bold text-2xl'>{totalQueries.toLocaleString()}</p>
              <p className='text-muted-foreground text-xs'>Total Queries</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card className='overflow-hidden'>
        <CardContent className='p-4'>
          <div className='flex items-center gap-3'>
            <TrendIcon isPositive={isPositive} isNegative={isNegative} />
            <div>
              <p
                className={cn(
                  "font-bold text-2xl",
                  isPositive && "text-green-400",
                  isNegative && "text-red-400"
                )}
              >
                {trend || "—"}
              </p>
              <p className='text-muted-foreground text-xs'>vs Last Period</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card className='overflow-hidden'>
        <CardContent className='p-4'>
          <div className='flex items-center gap-3'>
            <div className='rounded-lg bg-blue-500/10 p-2'>
              <LucideBot className='h-5 w-5 text-blue-400' />
            </div>
            <div>
              <p className='font-bold text-2xl'>{viaAgent}</p>
              <p className='text-muted-foreground text-xs'>Via Agent</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card className='overflow-hidden'>
        <CardContent className='p-4'>
          <div className='flex items-center gap-3'>
            <div className='rounded-lg bg-purple-500/10 p-2'>
              <LucideWorkflow className='h-5 w-5 text-purple-400' />
            </div>
            <div>
              <p className='font-bold text-2xl'>{viaWorkflow}</p>
              <p className='text-muted-foreground text-xs'>Via Workflow</p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
