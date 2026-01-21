import { cn } from "@/libs/shadcn/utils";
import type { MetricAnalytics } from "@/services/api/metrics";

interface MetricListItemProps {
  metric: MetricAnalytics;
  rank: number;
  maxCount: number;
  onClick: () => void;
}

export default function MetricListItem({
  metric,
  rank,
  maxCount,
  onClick,
}: MetricListItemProps) {
  const percentage = (metric.count / maxCount) * 100;

  return (
    <div
      onClick={onClick}
      className={cn(
        "flex items-center gap-4 p-3 rounded-lg border cursor-pointer transition-all",
        "hover:bg-accent hover:border-primary/50",
      )}
    >
      <div
        className={cn(
          "w-8 h-8 rounded-full flex items-center justify-center text-sm font-bold",
          rank === 1 && "bg-yellow-500 text-yellow-950",
          rank === 2 && "bg-slate-400 text-slate-950",
          rank === 3 && "bg-amber-600 text-amber-950",
          rank > 3 && "bg-muted text-muted-foreground",
        )}
      >
        {rank}
      </div>
      <div className="flex-1 min-w-0">
        <p className="font-medium text-sm truncate">{metric.name}</p>
        <p className="text-xs text-muted-foreground">
          Last:{" "}
          {metric.last_used
            ? new Date(metric.last_used).toLocaleDateString()
            : "—"}
        </p>
      </div>
      <div className="w-24 h-2 bg-muted rounded-full overflow-hidden">
        <div
          className="h-full bg-primary rounded-full"
          style={{ width: `${percentage}%` }}
        />
      </div>
      <div className="text-right w-20">
        <p className="font-semibold text-sm">{metric.count.toLocaleString()}</p>
        <p className="text-xs text-muted-foreground">queries</p>
      </div>
    </div>
  );
}
