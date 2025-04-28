import { useEffect, useRef } from "react";
import type { EChartsOption } from "echarts";
import { init, getInstanceByDom } from "echarts";
import theme from "./theme.json";

export const Echarts = ({
  options,
  isLoading,
}: {
  options: EChartsOption;
  isLoading: boolean;
}) => {
  const chartRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!chartRef.current) return;
    const chart = init(chartRef.current, theme);

    const resizeChart = () => chart.resize();
    window.addEventListener("resize", resizeChart);

    return () => {
      chart.dispose();
      window.removeEventListener("resize", resizeChart);
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
