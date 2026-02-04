import type { EChartsOption } from "echarts";
import MiniChart from "./MiniChart";

interface ChartCardProps {
  title: string;
  value: string | number;
  subtitle: string;
  options: EChartsOption;
  isLoading: boolean;
}

export default function ChartCard({ title, value, subtitle, options, isLoading }: ChartCardProps) {
  return (
    <div className='flex flex-col'>
      <div className='mb-1 flex items-baseline gap-2 px-3'>
        <span className='font-semibold text-lg'>{value}</span>
        {subtitle && <span className='text-muted-foreground text-xs'>{subtitle}</span>}
      </div>
      <MiniChart options={options} isLoading={isLoading} title={title} />
    </div>
  );
}
