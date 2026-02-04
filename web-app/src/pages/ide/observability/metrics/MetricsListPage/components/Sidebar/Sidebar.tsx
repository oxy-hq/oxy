import { Database, FileText, Loader2 } from "lucide-react";
import { useMemo } from "react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from "@/components/ui/shadcn/card";
import type { MetricAnalyticsResponse } from "@/services/api/metrics";
import { CONTEXT_TYPE_CONFIG, SOURCE_TYPE_CONFIG } from "../../constants";
import InsightsCard from "./InsightsCard";
import SourceDistribution from "./SourceDistribution";

interface SidebarProps {
  analyticsData: MetricAnalyticsResponse | undefined;
  isLoading: boolean;
}

export default function Sidebar({ analyticsData, isLoading }: SidebarProps) {
  const totalUsage = analyticsData?.total_queries || 0;
  const mostPopular = analyticsData?.most_popular || null;
  const mostPopularCount = analyticsData?.most_popular_count || 0;

  // Convert source type breakdown to stats format
  const sourceTypeStats = useMemo(() => {
    if (!analyticsData?.by_source_type) {
      return {
        agent: { count: 0, percentage: 0 },
        workflow: { count: 0, percentage: 0 }
      };
    }
    const total = analyticsData.by_source_type.agent + analyticsData.by_source_type.workflow;
    return {
      agent: {
        count: analyticsData.by_source_type.agent,
        percentage: total > 0 ? Math.round((analyticsData.by_source_type.agent / total) * 100) : 0
      },
      workflow: {
        count: analyticsData.by_source_type.workflow,
        percentage:
          total > 0 ? Math.round((analyticsData.by_source_type.workflow / total) * 100) : 0
      }
    };
  }, [analyticsData]);

  // Convert context type breakdown to stats format
  const contextTypeStats = useMemo(() => {
    if (!analyticsData?.by_context_type) {
      return {
        sql: { count: 0, percentage: 0 },
        question: { count: 0, percentage: 0 },
        response: { count: 0, percentage: 0 },
        semantic: { count: 0, percentage: 0 }
      };
    }
    const total =
      analyticsData.by_context_type.sql +
      analyticsData.by_context_type.semantic_query +
      analyticsData.by_context_type.question +
      analyticsData.by_context_type.response;
    return {
      sql: {
        count: analyticsData.by_context_type.sql,
        percentage: total > 0 ? Math.round((analyticsData.by_context_type.sql / total) * 100) : 0
      },
      question: {
        count: analyticsData.by_context_type.question,
        percentage:
          total > 0 ? Math.round((analyticsData.by_context_type.question / total) * 100) : 0
      },
      response: {
        count: analyticsData.by_context_type.response,
        percentage:
          total > 0 ? Math.round((analyticsData.by_context_type.response / total) * 100) : 0
      },
      semantic: {
        count: analyticsData.by_context_type.semantic_query,
        percentage:
          total > 0 ? Math.round((analyticsData.by_context_type.semantic_query / total) * 100) : 0
      }
    };
  }, [analyticsData]);

  return (
    <div className='space-y-6'>
      {/* Source Type Distribution */}
      <Card>
        <CardHeader className='pb-3'>
          <CardTitle className='flex items-center gap-2 text-base'>
            <Database className='h-4 w-4' />
            By Source Type
            {isLoading && <Loader2 className='h-3 w-3 animate-spin text-muted-foreground' />}
          </CardTitle>
          <CardDescription>How metrics are accessed</CardDescription>
        </CardHeader>
        <CardContent>
          <SourceDistribution stats={sourceTypeStats} config={SOURCE_TYPE_CONFIG} />
        </CardContent>
      </Card>

      {/* Context Type Distribution */}
      <Card>
        <CardHeader className='pb-3'>
          <CardTitle className='flex items-center gap-2 text-base'>
            <FileText className='h-4 w-4' />
            By Context Type
            {isLoading && <Loader2 className='h-3 w-3 animate-spin text-muted-foreground' />}
          </CardTitle>
          <CardDescription>What context was captured</CardDescription>
        </CardHeader>
        <CardContent>
          <SourceDistribution stats={contextTypeStats} config={CONTEXT_TYPE_CONFIG} />
        </CardContent>
      </Card>

      {/* Insights */}
      <InsightsCard
        mostPopular={mostPopular}
        mostPopularCount={mostPopularCount}
        totalUsage={totalUsage}
        agentPercentage={sourceTypeStats.agent?.percentage ?? 0}
        isLoading={isLoading}
      />
    </div>
  );
}
