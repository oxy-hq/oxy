// Cluster Analytics Types

export interface ClusterStats {
  totalClusters: number;
  totalQuestions: number;
  outlierCount: number;
  avgQuestionsPerCluster: number;
  answeredRate: number;
  failedRate: number;
  topClusterName: string;
  topClusterCount: number;
}

export interface ClusterBreakdown {
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

export type TimeRange = "7d" | "30d" | "90d";

export const TIME_RANGE_OPTIONS: { value: TimeRange; label: string }[] = [
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "90d", label: "90d" },
];

export function timeRangeToDays(range: TimeRange): number {
  switch (range) {
    case "7d":
      return 7;
    case "30d":
      return 30;
    case "90d":
      return 90;
  }
}

export const LIMIT_OPTIONS = [
  { value: 100, label: "100" },
  { value: 250, label: "250" },
  { value: 500, label: "500" },
  { value: 1000, label: "1000" },
];
