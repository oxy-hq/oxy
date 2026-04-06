import { Card, CardContent } from "@/components/ui/shadcn/card";
import { Spinner } from "@/components/ui/shadcn/spinner";
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
  variant = "default"
}: StatsCardProps) {
  const getIconBgColor = () => {
    switch (variant) {
      case "success":
        return "bg-success/10 text-success";
      case "warning":
        return "bg-warning/10 text-warning";
      case "danger":
        return "bg-destructive/10 text-destructive";
      default:
        return "bg-primary/10 text-primary";
    }
  };

  return (
    <Card className='overflow-hidden bg-transparent shadow-none'>
      <CardContent className='p-4'>
        <div className='flex items-start justify-between'>
          <div className='min-w-0 space-y-1'>
            <div className='flex items-center gap-2'>
              <p className='font-medium text-muted-foreground text-xs'>{title}</p>
              {isLoading && <Spinner className='size-3 text-muted-foreground' />}
            </div>
            <div className='flex items-baseline gap-2'>
              <p className='truncate font-bold text-2xl tracking-tight'>{value}</p>
              {trend && !isLoading && (
                <span
                  className={cn(
                    "font-medium text-xs",
                    trend.positive ? "text-success" : "text-destructive"
                  )}
                >
                  {trend.positive ? "+" : ""}
                  {trend.value}%
                </span>
              )}
            </div>
            <p className='text-muted-foreground text-xs'>{subtitle}</p>
          </div>
          <div className={cn("rounded-lg p-2", getIconBgColor())}>{icon}</div>
        </div>
      </CardContent>
    </Card>
  );
}
