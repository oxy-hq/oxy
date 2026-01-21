import { ArrowUpRight } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import { getRankColor } from "../../constants";
import type { MetricAnalytics } from "@/services/api/metrics";

interface MetricCardProps {
  metric: MetricAnalytics;
  rank: number;
  maxCount: number;
  onClick: () => void;
}

export default function MetricCard({
  metric,
  rank,
  maxCount,
  onClick,
}: MetricCardProps) {
  const percentage = (metric.count / maxCount) * 100;
  const isTop3 = rank <= 3;

  return (
    <div
      onClick={onClick}
      className={cn(
        "group relative p-4 rounded-xl border cursor-pointer transition-all duration-200",
        "hover:border-primary/50 hover:shadow-lg hover:shadow-primary/5",
        "bg-gradient-to-br from-card to-card/50",
        isTop3 && "ring-1 ring-primary/20",
      )}
    >
      {/* Rank badge */}
      <div
        className={cn(
          "absolute -top-2 -left-2 w-7 h-7 rounded-full flex items-center justify-center text-xs font-bold",
          rank === 1 && "bg-yellow-500 text-yellow-950",
          rank === 2 && "bg-slate-400 text-slate-950",
          rank === 3 && "bg-amber-600 text-amber-950",
          rank > 3 && "bg-muted text-muted-foreground",
        )}
      >
        {rank}
      </div>

      {/* Content */}
      <div className="space-y-3">
        <div className="flex items-start justify-between gap-2">
          <h3 className="font-semibold text-sm leading-tight line-clamp-2 group-hover:text-primary transition-colors">
            {metric.name}
          </h3>
          <ArrowUpRight className="h-4 w-4 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0" />
        </div>

        {/* Usage bar */}
        <div className="space-y-1">
          <div className="h-1.5 bg-muted rounded-full overflow-hidden">
            <div
              className={cn(
                "h-full rounded-full transition-all duration-500",
                getRankColor(rank),
              )}
              style={{ width: `${percentage}%` }}
            />
          </div>
          <div className="flex items-center justify-between text-xs">
            <span className="text-muted-foreground">
              {metric.last_used
                ? new Date(metric.last_used).toLocaleDateString()
                : "—"}
            </span>
            <span className="font-medium text-foreground">
              {metric.count.toLocaleString()} uses
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
