import { ExecutionSummary, EXECUTION_TYPES, ExecutionType } from "../types";
import {
  ShieldCheck,
  Sparkles,
  TrendingUp,
  CheckCircle,
  Activity,
  Loader2,
} from "lucide-react";
import { Card, CardContent } from "@/components/ui/shadcn/card";

interface SummaryCardsProps {
  summary: ExecutionSummary;
  isLoading: boolean;
}

function formatNumber(num: number): string {
  if (num >= 1000000) return `${(num / 1000000).toFixed(1)}M`;
  if (num >= 1000) return `${(num / 1000).toFixed(1)}K`;
  return num.toString();
}

function getExecutionTypeLabel(type: string): string {
  if (type === "none") return "None";
  const typeInfo = EXECUTION_TYPES[type as ExecutionType];
  return typeInfo?.label ?? type;
}

interface StatsCardProps {
  title: string;
  value: string | number;
  subtitle: string;
  icon: React.ReactNode;
  isLoading?: boolean;
}

function StatsCard({
  title,
  value,
  subtitle,
  icon,
  isLoading,
}: StatsCardProps) {
  return (
    <Card className="overflow-hidden">
      <CardContent className="p-4">
        <div className="flex items-start justify-between">
          <div className="space-y-1 min-w-0">
            <div className="flex items-center gap-2">
              <p className="text-xs font-medium text-muted-foreground">
                {title}
              </p>
              {isLoading && (
                <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />
              )}
            </div>
            <p className="text-2xl font-bold tracking-tight truncate">
              {value}
            </p>
            <p className="text-xs text-muted-foreground">{subtitle}</p>
          </div>
          <div className="p-2 rounded-lg bg-primary/10 text-primary">
            {icon}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

export default function SummaryCards({
  summary,
  isLoading,
}: SummaryCardsProps) {
  return (
    <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-4">
      <StatsCard
        title="Total Executions"
        value={formatNumber(summary.totalExecutions)}
        subtitle="queries & workflows"
        icon={<Activity className="h-5 w-5" />}
        isLoading={isLoading}
      />
      <StatsCard
        title="Verified"
        value={`${summary.verifiedPercent.toFixed(1)}%`}
        subtitle={`${formatNumber(summary.verifiedCount)} executions`}
        icon={<ShieldCheck className="h-5 w-5" />}
        isLoading={isLoading}
      />
      <StatsCard
        title="Generated"
        value={`${summary.generatedPercent.toFixed(1)}%`}
        subtitle={`${formatNumber(summary.generatedCount)} executions`}
        icon={<Sparkles className="h-5 w-5" />}
        isLoading={isLoading}
      />
      <StatsCard
        title="Most Executed"
        value={getExecutionTypeLabel(summary.mostExecutedType)}
        subtitle="most common type"
        icon={<TrendingUp className="h-5 w-5" />}
        isLoading={isLoading}
      />
      <StatsCard
        title="Success Rate"
        value={`${summary.successRateVerified.toFixed(1)}%`}
        subtitle={`verified (${summary.successRateGenerated.toFixed(1)}% generated)`}
        icon={<CheckCircle className="h-5 w-5" />}
        isLoading={isLoading}
      />
    </div>
  );
}
