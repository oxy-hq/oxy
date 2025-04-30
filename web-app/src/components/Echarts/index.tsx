import { useCallback, useEffect, useRef } from "react";
import type { EChartsOption } from "echarts";
import { init, getInstanceByDom } from "echarts";
import theme from "./theme.json";
import { useResizeDetector } from "react-resize-detector";

export const Echarts = ({
  options,
  isLoading,
}: {
  options: EChartsOption;
  isLoading: boolean;
}) => {
  const chartRef = useRef<HTMLDivElement>(null);
  const onResize = useCallback(() => {
    if (chartRef.current) {
      const chart = getInstanceByDom(chartRef.current);
      chart?.resize();
    }
  }, [chartRef]);

  useResizeDetector({
    targetRef: chartRef,
    onResize: onResize,
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
      chart?.setOption(options);
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

  return <div ref={chartRef} style={{ width: "100%", height: "400px" }} />;
};
