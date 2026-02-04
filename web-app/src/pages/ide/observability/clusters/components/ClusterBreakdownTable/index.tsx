import { CheckCircle, Hash, Layers, Loader2, MessageSquare, Quote, XCircle } from "lucide-react";
import { useMemo } from "react";
import { Badge } from "@/components/ui/shadcn/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from "@/components/ui/shadcn/card";
import { cn } from "@/libs/shadcn/utils";
import type { ClusterMapPoint, ClusterSummary } from "@/services/api/traces";

interface ClusterBreakdownTableProps {
  clusters: ClusterSummary[];
  points: ClusterMapPoint[];
  isLoading: boolean;
  onClusterClick?: (clusterId: number) => void;
}

interface ClusterStats {
  clusterId: number;
  intentName: string;
  description: string;
  count: number;
  answeredCount: number;
  failedCount: number;
  successRate: number;
  color: string;
  sampleQuestions: string[];
}

function getSuccessRateBadgeColor(successRate: number): string {
  if (successRate >= 90) {
    return "bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400";
  }
  if (successRate >= 70) {
    return "bg-yellow-100 text-yellow-800 dark:bg-yellow-900/30 dark:text-yellow-400";
  }
  return "bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400";
}

function DistributionBar({ success, error }: { success: number; error: number }) {
  return (
    <div className='flex h-2 w-full overflow-hidden rounded-full bg-muted'>
      <div
        className='bg-emerald-500 transition-all'
        style={{ width: `${success}%` }}
        title={`Success: ${success.toFixed(1)}%`}
      />
      <div
        className='bg-rose-500 transition-all'
        style={{ width: `${error}%` }}
        title={`Error: ${error.toFixed(1)}%`}
      />
    </div>
  );
}

export default function ClusterBreakdownTable({
  clusters,
  points,
  isLoading
}: ClusterBreakdownTableProps) {
  const clusterStats = useMemo(() => {
    // Group points by cluster
    const pointsByCluster = new Map<number, ClusterMapPoint[]>();
    points.forEach((point) => {
      const existing = pointsByCluster.get(point.clusterId) || [];
      existing.push(point);
      pointsByCluster.set(point.clusterId, existing);
    });

    // Calculate stats for each cluster
    const stats: ClusterStats[] = clusters
      .filter((c) => c.clusterId !== -1)
      .map((cluster) => {
        const clusterPoints = pointsByCluster.get(cluster.clusterId) || [];
        let answeredCount = 0;
        let failedCount = 0;

        clusterPoints.forEach((point) => {
          // Use status from API if available
          if (point.status === "ok") {
            answeredCount++;
          } else if (point.status === "error") {
            failedCount++;
          }
        });

        const total = clusterPoints.length;
        const successRate = total > 0 ? (answeredCount / total) * 100 : 0;

        return {
          clusterId: cluster.clusterId,
          intentName: cluster.intentName,
          description: cluster.description,
          count: cluster.count,
          answeredCount,
          failedCount,
          successRate,
          color: cluster.color,
          sampleQuestions: cluster.sampleQuestions
        };
      })
      .sort((a, b) => b.count - a.count);

    return stats;
  }, [clusters, points]);

  if (isLoading) {
    return (
      <Card>
        <CardHeader>
          <div className='flex items-center gap-2'>
            <Layers className='h-5 w-5 text-primary' />
            <CardTitle>Cluster Breakdown</CardTitle>
            <Loader2 className='h-4 w-4 animate-spin text-muted-foreground' />
          </div>
          <CardDescription>Performance metrics by cluster</CardDescription>
        </CardHeader>
        <CardContent>
          <div className='animate-pulse space-y-2'>
            {[1, 2, 3].map((i) => (
              <div key={i} className='h-10 rounded bg-muted' />
            ))}
          </div>
        </CardContent>
      </Card>
    );
  }

  if (clusterStats.length === 0) {
    return (
      <Card>
        <CardHeader>
          <div className='flex items-center gap-2'>
            <Layers className='h-5 w-5 text-primary' />
            <CardTitle>Cluster Breakdown</CardTitle>
          </div>
          <CardDescription>Performance metrics by cluster</CardDescription>
        </CardHeader>
        <CardContent>
          <p className='py-4 text-center text-muted-foreground text-sm'>
            No cluster data available
          </p>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader>
        <div className='flex items-center gap-2'>
          <Layers className='h-5 w-5 text-primary' />
          <CardTitle>Cluster Breakdown</CardTitle>
        </div>
        <CardDescription>Performance metrics by cluster</CardDescription>
      </CardHeader>
      <CardContent>
        <div className='space-y-4'>
          {clusterStats.map((cluster, index) => (
            <div
              key={cluster.clusterId}
              className='relative rounded-xl border bg-card p-5 transition-colors hover:bg-muted/30'
            >
              {/* Top Row: Color badge, Intent Name, Tags */}
              <div className='mb-3 flex items-start justify-between gap-4'>
                <div className='flex min-w-0 flex-1 items-center gap-3'>
                  <div
                    className='h-3 w-3 shrink-0 rounded-full'
                    style={{ backgroundColor: cluster.color }}
                  />
                  <div className='flex flex-wrap items-center gap-2'>
                    <h4 className='font-semibold text-base'>{cluster.intentName}</h4>
                    <Badge variant='outline' className='font-normal text-xs'>
                      <Hash className='mr-1 h-3 w-3' />
                      {cluster.clusterId}
                    </Badge>
                    <Badge variant='outline' className='font-normal text-xs'>
                      <MessageSquare className='mr-1 h-3 w-3' />
                      {cluster.count} questions
                    </Badge>
                  </div>
                </div>
                {/* Success Rate Badge - prominent on the right */}
                <Badge
                  className={cn(
                    "shrink-0 px-3 py-1 font-semibold text-sm",
                    getSuccessRateBadgeColor(cluster.successRate)
                  )}
                >
                  {cluster.successRate.toFixed(1)}% success
                </Badge>
              </div>

              {/* Description - Full display */}
              {cluster.description && (
                <p className='mb-4 text-muted-foreground text-sm leading-relaxed'>
                  {cluster.description}
                </p>
              )}

              {/* Success/Error Stats */}
              <div className='mb-4 flex items-center gap-6 text-sm'>
                <div className='flex items-center gap-1.5'>
                  <CheckCircle className='h-4 w-4 text-emerald-500' />
                  <span className='font-medium text-emerald-600 dark:text-emerald-400'>
                    {cluster.answeredCount}
                  </span>
                  <span className='text-muted-foreground'>success</span>
                </div>
                <div className='flex items-center gap-1.5'>
                  <XCircle className='h-4 w-4 text-rose-500' />
                  <span className='font-medium text-rose-600 dark:text-rose-400'>
                    {cluster.failedCount}
                  </span>
                  <span className='text-muted-foreground'>error</span>
                </div>
                <div className='flex flex-1 items-center gap-3'>
                  <DistributionBar
                    success={cluster.count > 0 ? (cluster.answeredCount / cluster.count) * 100 : 0}
                    error={cluster.count > 0 ? (cluster.failedCount / cluster.count) * 100 : 0}
                  />
                  <span className='shrink-0 text-muted-foreground text-xs'>
                    {cluster.count > 0
                      ? ((cluster.answeredCount / cluster.count) * 100).toFixed(0)
                      : 0}
                    % /{" "}
                    {cluster.count > 0
                      ? ((cluster.failedCount / cluster.count) * 100).toFixed(0)
                      : 0}
                    %
                  </span>
                </div>
              </div>

              {/* Sample Questions */}
              {cluster.sampleQuestions.length > 0 && (
                <div className='mt-4 border-t pt-4'>
                  {(() => {
                    const uniqueQuestions = [...new Set(cluster.sampleQuestions)];
                    return (
                      <>
                        <div className='mb-2 flex items-center gap-2 text-muted-foreground text-xs'>
                          <Quote className='h-3.5 w-3.5' />
                          <span>Sample Questions ({uniqueQuestions.length})</span>
                        </div>
                        <div className='flex flex-col gap-2'>
                          {uniqueQuestions.map((q, idx) => (
                            <Badge
                              key={idx}
                              variant='secondary'
                              className='w-fit max-w-full px-3 py-1.5 font-normal text-xs'
                            >
                              <span className='truncate'>{q}</span>
                            </Badge>
                          ))}
                        </div>
                      </>
                    );
                  })()}
                </div>
              )}

              {/* Rank indicator */}
              {index < 3 && (
                <div className='absolute -top-2 -right-2'>
                  <Badge
                    className={cn(
                      "font-bold text-xs",
                      index === 0 && "bg-yellow-500 text-yellow-950",
                      index === 1 && "bg-slate-400 text-slate-950",
                      index === 2 && "bg-amber-600 text-amber-950"
                    )}
                  >
                    #{index + 1}
                  </Badge>
                </div>
              )}
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
