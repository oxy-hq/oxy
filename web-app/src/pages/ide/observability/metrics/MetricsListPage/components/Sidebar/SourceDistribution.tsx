import { cn } from "@/libs/shadcn/utils";

interface DistributionStats {
  count: number;
  percentage: number;
}

interface DistributionConfig {
  label: string;
  color: string;
  bgColor: string;
  icon: React.ReactNode;
}

interface SourceDistributionProps {
  stats: Record<string, DistributionStats>;
  config: Record<string, DistributionConfig>;
}

export default function SourceDistribution({
  stats,
  config: configMap,
}: SourceDistributionProps) {
  return (
    <div className="space-y-3">
      {Object.entries(stats).map(([type, data]) => {
        const config = configMap[type];
        if (!config) return null;

        return (
          <div key={type} className="flex items-center gap-3">
            <div className={cn("p-1.5 rounded-md", config.bgColor)}>
              <span className={config.color}>{config.icon}</span>
            </div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between text-sm mb-1">
                <span className="font-medium">{config.label}</span>
                <span className="text-muted-foreground">
                  {data.count.toLocaleString()}
                </span>
              </div>
              <div className="h-1.5 bg-muted rounded-full overflow-hidden">
                <div
                  className={cn(
                    "h-full rounded-full",
                    config.bgColor.replace("/10", ""),
                  )}
                  style={{ width: `${data.percentage}%` }}
                />
              </div>
            </div>
            <span className="text-xs text-muted-foreground w-10 text-right">
              {data.percentage}%
            </span>
          </div>
        );
      })}
    </div>
  );
}
