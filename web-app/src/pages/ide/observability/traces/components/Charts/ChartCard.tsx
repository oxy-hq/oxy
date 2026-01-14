import type { EChartsOption } from "echarts";
import MiniChart from "./MiniChart";

interface ChartCardProps {
  title: string;
  value: string | number;
  subtitle: string;
  options: EChartsOption;
  isLoading: boolean;
}

export default function ChartCard({
  title,
  value,
  subtitle,
  options,
  isLoading,
}: ChartCardProps) {
  return (
    <div className="flex flex-col">
      <div className="flex items-baseline gap-2 mb-1 px-3">
        <span className="text-lg font-semibold">{value}</span>
        {subtitle && (
          <span className="text-xs text-muted-foreground">{subtitle}</span>
        )}
      </div>
      <MiniChart options={options} isLoading={isLoading} title={title} />
    </div>
  );
}
