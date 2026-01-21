import { Card, CardContent } from "@/components/ui/shadcn/card";
import { Loader2 } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";

interface StatsCardProps {
  title: string;
  value: string | number;
  subtitle: string;
  icon: React.ReactNode;
  trend?: { value: number; positive: boolean };
  isLoading?: boolean;
  variant?: "default" | "success" | "warning" | "danger";
}

export default function StatsCard({
  title,
  value,
  subtitle,
  icon,
  trend,
  isLoading,
  variant = "default",
}: StatsCardProps) {
  const getIconBgColor = () => {
    switch (variant) {
      case "success":
        return "bg-emerald-500/10 text-emerald-500";
      case "warning":
        return "bg-amber-500/10 text-amber-500";
      case "danger":
        return "bg-rose-500/10 text-rose-500";
      default:
        return "bg-primary/10 text-primary";
    }
  };

  return (
    <Card className="overflow-hidden">
      <CardContent className="p-4">
        <div className="flex items-start justify-between">
          <div className="space-y-1">
            <div className="flex items-center gap-2">
              <p className="text-xs font-medium text-muted-foreground">
                {title}
              </p>
              {isLoading && (
                <Loader2 className="h-3 w-3 animate-spin text-muted-foreground" />
              )}
            </div>
            <div className="flex items-baseline gap-2">
              <p className="text-2xl font-bold tracking-tight">{value}</p>
              {trend && !isLoading && (
                <span
                  className={cn(
                    "text-xs font-medium",
                    trend.positive ? "text-green-400" : "text-red-400",
                  )}
                >
                  {trend.positive ? "+" : ""}
                  {trend.value}%
                </span>
              )}
            </div>
            <p className="text-xs text-muted-foreground">{subtitle}</p>
          </div>
          <div className={cn("p-2 rounded-lg", getIconBgColor())}>{icon}</div>
        </div>
      </CardContent>
    </Card>
  );
}
