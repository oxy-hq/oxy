import { BarChart3, TrendingUp, Users, Zap } from "lucide-react";
import type { MetricAnalyticsResponse } from "@/services/api/metrics";
import type { DaysValue } from "../../constants";
import StatsCard from "./StatsCard";

interface StatsRowProps {
  analyticsData: MetricAnalyticsResponse | undefined;
  daysFilter: DaysValue;
  isLoading: boolean;
}

export default function StatsRow({ analyticsData, daysFilter, isLoading }: StatsRowProps) {
  const totalUsage = analyticsData?.total_queries || 0;
  const uniqueMetrics = analyticsData?.unique_metrics || 0;
  const mostPopular = analyticsData?.most_popular || null;
  const mostPopularCount = analyticsData?.most_popular_count || 0;
  const avgPerMetric = analyticsData?.avg_per_metric || 0;
  const trendVsLastPeriod = analyticsData?.trend_vs_last_period || null;
  const parseTrend = () => {
    if (!trendVsLastPeriod) return undefined;
    const isPositive = trendVsLastPeriod.startsWith("+");
    const value = parseInt(trendVsLastPeriod.replace(/[+\-%]/g, ""), 10);
    return { value, positive: isPositive };
  };

  return (
    <div className='grid grid-cols-1 gap-4 md:grid-cols-4'>
      <StatsCard
        title='Total Queries'
        value={totalUsage.toLocaleString()}
        subtitle={`Last ${daysFilter} days`}
        icon={<Zap className='h-5 w-5' />}
        trend={parseTrend()}
        isLoading={isLoading}
      />
      <StatsCard
        title='Unique Metrics Used'
        value={uniqueMetrics}
        subtitle='Tracked metrics'
        icon={<BarChart3 className='h-5 w-5' />}
        isLoading={isLoading}
      />
      <StatsCard
        title='Most Popular'
        value={mostPopular || "-"}
        subtitle={`${mostPopularCount.toLocaleString()} queries`}
        icon={<TrendingUp className='h-5 w-5' />}
        isLoading={isLoading}
      />
      <StatsCard
        title='Avg. Per Metric'
        value={Math.round(avgPerMetric).toLocaleString()}
        subtitle='Queries per metric'
        icon={<Users className='h-5 w-5' />}
        isLoading={isLoading}
      />
    </div>
  );
}
