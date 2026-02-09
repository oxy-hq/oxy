import type { EChartsOption } from "echarts";
import { getInstanceByDom, init } from "echarts";
import { useCallback, useEffect, useRef } from "react";
import { useResizeDetector } from "react-resize-detector";
import theme from "./theme.json";

export const Echarts = ({
  options,
  isLoading,
  title,
  testId
}: {
  options: EChartsOption;
  isLoading: boolean;
  title?: string;
  testId?: string;
}) => {
  const chartRef = useRef<HTMLDivElement>(null);
  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
  const onResize = useCallback(() => {
    if (chartRef.current) {
      const chart = getInstanceByDom(chartRef.current);
      chart?.resize();
    }
  }, [chartRef]);

  useResizeDetector({
    targetRef: chartRef,
    onResize: onResize
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

      chart?.setOption(
        {
          ...options,
          toolbox: options.toolbox || {
            feature: {
              dataZoom: {
                yAxisIndex: "none"
              },
              saveAsImage: {}
            }
          }
        },
        true
      );
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
    <div data-testid={testId}>
      {title && <h2 className='font-bold text-foreground text-xl'>{title}</h2>}
      <div ref={chartRef} style={{ width: "100%", height: "400px" }} />
    </div>
  );
};
