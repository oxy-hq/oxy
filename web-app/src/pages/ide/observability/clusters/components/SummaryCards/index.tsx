import { useMemo } from "react";
import {
  Layers,
  Users,
  AlertTriangle,
  CheckCircle,
  Target,
} from "lucide-react";
import StatsCard from "./StatsCard";
import type { ClusterSummary, ClusterMapPoint } from "@/services/api/traces";

interface SummaryCardsProps {
  clusters: ClusterSummary[];
  points: ClusterMapPoint[];
  isLoading: boolean;
}

function formatNumber(num: number): string {
  if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
  if (num >= 1000) return `${(num / 1000).toFixed(1)}K`;
  return num.toString();
}

export default function SummaryCards({
  clusters,
  points,
  isLoading,
}: SummaryCardsProps) {
  const stats = useMemo(() => {
    const validClusters = clusters.filter((c) => c.clusterId !== -1);
    const outlierCluster = clusters.find((c) => c.clusterId === -1);
    const outlierCount = outlierCluster?.count ?? 0;

    // Calculate success rate using status field from API
    let successCount = 0;
    points.forEach((point) => {
      if (point.status === "ok") {
        successCount++;
      }
    });

    const totalPoints = points.length;
    const successRate =
      totalPoints > 0 ? (successCount / totalPoints) * 100 : 0;

    // Top cluster
    const topCluster = validClusters.reduce(
      (max, c) => (c.count > (max?.count ?? 0) ? c : max),
      null as ClusterSummary | null,
    );

    return {
      totalClusters: validClusters.length,
      totalQuestions: totalPoints,
      outlierCount,
      successRate: successRate.toFixed(1),
      topClusterName: topCluster?.intentName ?? "-",
      topClusterCount: topCluster?.count ?? 0,
    };
  }, [clusters, points]);

  return (
    <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-4">
      <StatsCard
        title="Top Cluster"
        value={stats.topClusterName}
        subtitle={`${formatNumber(stats.topClusterCount)} queries`}
        icon={<Target className="h-5 w-5" />}
        isLoading={isLoading}
      />
      <StatsCard
        title="Clusters Found"
        value={stats.totalClusters}
        subtitle="semantic groups"
        icon={<Layers className="h-5 w-5" />}
        isLoading={isLoading}
      />
      <StatsCard
        title="Total Questions"
        value={formatNumber(stats.totalQuestions)}
        subtitle="sampled traces"
        icon={<Users className="h-5 w-5" />}
        isLoading={isLoading}
      />
      <StatsCard
        title="Outliers"
        value={formatNumber(stats.outlierCount)}
        subtitle="unclustered queries"
        icon={<AlertTriangle className="h-5 w-5" />}
        isLoading={isLoading}
        variant="warning"
      />
      <StatsCard
        title="Success Rate"
        value={`${stats.successRate}%`}
        subtitle="successfully answered"
        icon={<CheckCircle className="h-5 w-5" />}
        isLoading={isLoading}
        variant="success"
      />
    </div>
  );
}
