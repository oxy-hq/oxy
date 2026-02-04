import type { EChartsOption } from "echarts";
import { getInstanceByDom, init } from "echarts";
import { useCallback, useEffect, useRef } from "react";
import { useResizeDetector } from "react-resize-detector";
import theme from "@/components/Echarts/theme.json";

interface MiniChartProps {
  options: EChartsOption;
  isLoading: boolean;
  title: string;
}

export default function MiniChart({ options, isLoading }: MiniChartProps) {
  const chartRef = useRef<HTMLDivElement>(null);

  const onResize = useCallback(() => {
    if (chartRef.current) {
      const chart = getInstanceByDom(chartRef.current);
      chart?.resize();
    }
  }, []);

  useResizeDetector({
    targetRef: chartRef,
    onResize
  });

  useEffect(() => {
    if (!chartRef.current) return;
    const chart = init(chartRef.current, theme);

    return () => {
      chart.dispose();
    };
  }, []);

  useEffect(() => {
    if (chartRef.current) {
      const chart = getInstanceByDom(chartRef.current);
      chart?.setOption(options, true);
      chart?.resize();
    }
  }, [options]);

  useEffect(() => {
    if (chartRef.current) {
      const chart = getInstanceByDom(chartRef.current);
      if (isLoading) {
        chart?.showLoading();
      } else {
        chart?.hideLoading();
      }
    }
  }, [isLoading]);

  return (
    <div className='flex flex-col rounded-lg border border-border bg-card p-3'>
      <div ref={chartRef} className='h-[100px] w-full' />
    </div>
  );
}
