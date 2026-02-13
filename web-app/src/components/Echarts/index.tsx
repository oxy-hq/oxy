import type { EChartsOption } from "echarts";
import { getInstanceByDom, init } from "echarts";
import { useCallback, useEffect, useRef } from "react";
import { useResizeDetector } from "react-resize-detector";
import { useSearchParams } from "react-router-dom";
import theme from "./theme.json";

export const Echarts = ({
  options,
  isLoading,
  title,
  testId,
  chartIndex,
}: {
  options: EChartsOption;
  isLoading: boolean;
  title?: string;
  testId?: string;
  chartIndex?: number;
}) => {
  const chartRef = useRef<HTMLDivElement>(null);
  const [searchParams] = useSearchParams();
  // biome-ignore lint/correctness/useExhaustiveDependencies: <explanation>
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

  const isExportMode = searchParams.get("export") === "true";

  useEffect(() => {
    if (chartRef.current) {
      const chart = getInstanceByDom(chartRef.current);

      // Hide toolbox in export mode to avoid buttons in exported image
      const toolboxConfig = isExportMode
        ? { show: false }
        : options.toolbox || {
            feature: {
              dataZoom: {
                yAxisIndex: "none",
              },
              saveAsImage: {},
            },
          };

      chart?.setOption(
        {
          ...options,
          toolbox: toolboxConfig,
        },
        true,
      );
      chart?.resize();
    }
  }, [options, isExportMode]);

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

  const handleExportChart = useCallback(() => {
    if (!chartRef.current) return;

    const chart = getInstanceByDom(chartRef.current);
    if (!chart) return;

    const doExport = () => {
      try {
        const option = chart.getOption();

        let chartName = title || `chart-${Date.now()}`;
        if (!title && option?.title) {
          const titleArray = option.title as Array<{ text?: string }>;
          if (titleArray[0]?.text) {
            chartName = titleArray[0].text
              .toLowerCase()
              .replace(/[^a-z0-9]+/g, "-")
              .replace(/^-+|-+$/g, "");
          }
        }

        const imageData = chart.getDataURL({
          type: "png",
          pixelRatio: 2,
          backgroundColor: "#212121",
        });

        // Create individual DOM element for this chart with index
        const resultEl = document.createElement("div");
        resultEl.className = `chart-export-result chart-export-result-${chartIndex ?? 0}`;
        resultEl.style.display = "none";
        resultEl.setAttribute(
          "data-chart",
          JSON.stringify({
            name: chartName,
            index: chartIndex ?? 0,
            imageData,
          }),
        );
        resultEl.setAttribute("data-ready", "true");
        resultEl.setAttribute("data-index", String(chartIndex ?? 0));
        document.body.appendChild(resultEl);

        console.log(`Chart exported: ${chartName} (index: ${chartIndex ?? 0})`);
      } catch (error) {
        console.error("Failed to export chart:", error);
      }
    };

    // Wait for chart to finish rendering using 'finished' event
    // 'finished' fires when initial animation and bindings are complete
    let exported = false;

    const onFinished = () => {
      if (exported) return;
      exported = true;
      chart.off("finished", onFinished);
      doExport();
    };

    chart.on("finished", onFinished);

    // Fallback: if chart is already rendered or 'finished' doesn't fire
    // (e.g., animation disabled), export after a short delay
    setTimeout(() => {
      if (!exported) {
        exported = true;
        chart.off("finished", onFinished);
        doExport();
      }
    }, 500);
  }, [title, chartIndex]);

  return (
    <div
      data-testid={testId}
      className="chart-wrapper"
      data-chart-index={chartIndex ?? 0}
    >
      {title && <h2 className="font-bold text-foreground text-xl">{title}</h2>}
      <div ref={chartRef} style={{ width: "100%", height: "400px" }} />
      {isExportMode && !isLoading && (
        <button
          className={`chart-export-trigger chart-export-trigger-${chartIndex ?? 0}`}
          onClick={handleExportChart}
          data-chart-index={chartIndex ?? 0}
          style={{ display: "none" }}
          type="button"
        >
          Export Chart
        </button>
      )}
    </div>
  );
};
