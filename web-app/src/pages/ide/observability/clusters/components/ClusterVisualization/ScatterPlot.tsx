import { type ECharts, type EChartsOption, init as initEcharts } from "echarts";
import { useEffect, useMemo, useRef } from "react";
import { ErrorBoundary } from "react-error-boundary";
import { resolveColor } from "@/components/Echarts/resolveColor";
import ErrorAlert from "@/components/ui/ErrorAlert";
import type { ClusterMapPoint, ClusterSummary } from "@/services/api/traces";
import { formatTimeAgo } from "../../../utils";

interface ScatterPlotProps {
  points: ClusterMapPoint[];
  getPointColor: (point: ClusterMapPoint) => string;
  clusters: ClusterSummary[];
  onPointClick: (point: ClusterMapPoint) => void;
  selectedPoint: ClusterMapPoint | null;
}

function ScatterPlotInner({
  points,
  getPointColor,
  clusters,
  onPointClick,
  selectedPoint
}: ScatterPlotProps) {
  const chartRef = useRef<HTMLDivElement>(null);
  const chartInstance = useRef<ECharts | null>(null);

  // Memoize series data without selectedPoint dependency
  const seriesData = useMemo(() => {
    return buildSeriesData(points, clusters);
  }, [points, clusters]);

  useEffect(() => {
    if (!chartRef.current) return;

    chartInstance.current = initEcharts(chartRef.current, undefined, {
      renderer: "canvas"
    });

    const handleResize = () => {
      chartInstance.current?.resize();
    };

    window.addEventListener("resize", handleResize);

    // Use ResizeObserver to handle container size changes
    const resizeObserver = new ResizeObserver(() => {
      chartInstance.current?.resize();
    });

    resizeObserver.observe(chartRef.current);

    return () => {
      window.removeEventListener("resize", handleResize);
      resizeObserver.disconnect();
      chartInstance.current?.dispose();
    };
  }, []);

  // Auto-resize when clusters change (hide/show) or container size changes
  useEffect(() => {
    if (chartInstance.current) {
      // Small delay to ensure DOM updates are complete
      const timeoutId = setTimeout(() => {
        chartInstance.current?.resize();
      }, 10);

      return () => clearTimeout(timeoutId);
    }
  }, []);

  useEffect(() => {
    if (!chartInstance.current) return;

    const option = buildChartOption(seriesData, getPointColor);
    chartInstance.current.setOption(option, true);

    chartInstance.current.off("click");
    chartInstance.current.on("click", (params: unknown) => {
      const p = params as { data?: { point?: ClusterMapPoint } };
      const point = p.data?.point;
      if (point) {
        onPointClick(point);
      }
    });
  }, [seriesData, getPointColor, onPointClick]);

  // Handle selected point highlight separately to avoid rebuilding all series
  useEffect(() => {
    if (!chartInstance.current) return;

    // Always clear previous highlights first
    chartInstance.current.dispatchAction({
      type: "downplay"
    });

    if (selectedPoint) {
      for (let seriesIndex = 0; seriesIndex < seriesData.length; seriesIndex++) {
        const series = seriesData[seriesIndex];
        const dataIndex = series.data.findIndex((d) => d.point.traceId === selectedPoint.traceId);
        if (dataIndex !== -1) {
          chartInstance.current.dispatchAction({
            type: "highlight",
            seriesIndex,
            dataIndex
          });
          break;
        }
      }
    }
  }, [selectedPoint, seriesData]);

  return <div ref={chartRef} className='h-full w-full' />;
}

function buildSeriesData(points: ClusterMapPoint[], clusters: ClusterSummary[]) {
  // Create a lookup map for clusters for O(1) access
  const clusterMap = new Map(clusters.map((c) => [c.clusterId, c]));
  const grouped = new Map<number, ClusterMapPoint[]>();

  for (const point of points) {
    const existing = grouped.get(point.clusterId);
    if (existing) {
      existing.push(point);
    } else {
      grouped.set(point.clusterId, [point]);
    }
  }

  return Array.from(grouped.entries()).map(([clusterId, clusterPoints]) => {
    const cluster = clusterMap.get(clusterId);
    const color = cluster?.color || resolveColor("--muted-foreground");
    const name = cluster?.intentName || "Unknown";

    return {
      name,
      type: "scatter" as const,
      symbolSize: 10, // Static size - much faster than function
      large: true, // Enable large mode for better performance
      largeThreshold: 100,
      data: clusterPoints.map((p) => ({
        value: [p.x, p.y],
        point: p
      })),
      itemStyle: {
        color,
        borderColor: resolveColor("--background"),
        borderWidth: 1
        // Removed shadowBlur/shadowColor - expensive to render
      },
      emphasis: {
        itemStyle: {
          borderColor: resolveColor("--background"),
          borderWidth: 2
        },
        scale: 1.5
      }
    };
  });
}

function buildChartOption(
  seriesData: ReturnType<typeof buildSeriesData>,
  getPointColor: (point: ClusterMapPoint) => string
): EChartsOption {
  return {
    animation: false,
    backgroundColor: "transparent",
    title: {
      text: "Semantic Cluster Map",
      subtext:
        "Points represent user queries, positioned by semantic similarity using dimensionality reduction",
      left: "center",
      top: 8,
      textStyle: { color: resolveColor("--muted-foreground"), fontSize: 14, fontWeight: "normal" },
      subtextStyle: { color: resolveColor("--muted-foreground"), fontSize: 11 }
    },
    grid: {
      left: 60,
      right: 40,
      top: 70,
      bottom: 80,
      containLabel: false
    },
    xAxis: {
      type: "value",
      scale: true,
      name: "Latent Dimension 1",
      nameLocation: "middle",
      nameGap: 30,
      nameTextStyle: { color: resolveColor("--muted-foreground"), fontSize: 12 },
      axisLine: { lineStyle: { color: resolveColor("--muted-foreground") } },
      splitLine: {
        lineStyle: { color: resolveColor("--border"), type: "dashed" }
      },
      axisLabel: { color: resolveColor("--muted-foreground") }
    },
    yAxis: {
      type: "value",
      scale: true,
      name: "Latent Dimension 2",
      nameLocation: "middle",
      nameGap: 45,
      nameTextStyle: { color: resolveColor("--muted-foreground"), fontSize: 12 },
      axisLine: { lineStyle: { color: resolveColor("--muted-foreground") } },
      splitLine: {
        lineStyle: { color: resolveColor("--border"), type: "dashed" }
      },
      axisLabel: { color: resolveColor("--muted-foreground") }
    },
    tooltip: {
      trigger: "item",
      backgroundColor: resolveColor("--popover"),
      borderColor: resolveColor("--border"),
      borderWidth: 1,
      padding: [12, 16],
      textStyle: { color: resolveColor("--popover-foreground"), fontSize: 13 },
      confine: true, // Keep tooltip within chart bounds
      appendToBody: true, // Render outside chart container for better perf
      formatter: (params: unknown) => {
        return formatTooltip(params, getPointColor);
      }
    },
    legend: {
      show: true,
      type: "scroll",
      orient: "horizontal",
      bottom: 8,
      left: "center",
      textStyle: { color: resolveColor("--muted-foreground"), fontSize: 11 },
      pageTextStyle: { color: resolveColor("--muted-foreground") },
      itemWidth: 12,
      itemHeight: 12,
      itemGap: 20
    },
    dataZoom: [
      { type: "inside", xAxisIndex: 0, filterMode: "none" },
      { type: "inside", yAxisIndex: 0, filterMode: "none" }
    ],
    series: seriesData
  };
}

function formatTooltip(params: unknown, getPointColor: (point: ClusterMapPoint) => string): string {
  const p = params as { data?: { point?: ClusterMapPoint } };
  const point = p.data?.point;
  if (!point) return "";

  const confidence = (point.confidence * 100).toFixed(1);
  const timeAgo = point.timestamp ? formatTimeAgo(point.timestamp) : "";

  return `
    <div style="max-width: 300px;">
      <div style="display: flex; align-items: center; gap: 8px; margin-bottom: 8px;">
        <span style="background: ${getPointColor(point)}; color: white; padding: 2px 8px; border-radius: 4px; font-size: 12px;">
          ${point.intentName}
        </span>
        <span style="color: var(--muted-foreground); font-size: 12px;">${confidence}%</span>
      </div>
      <div style="font-size: 13px; line-height: 1.4; color: var(--popover-foreground); overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 3; -webkit-box-orient: vertical;">
        ${point.question}
      </div>
      ${timeAgo ? `<div style="color: var(--muted-foreground); font-size: 11px; margin-top: 8px;">${timeAgo}</div>` : ""}
    </div>
  `;
}

function ScatterPlot(props: ScatterPlotProps) {
  return (
    <ErrorBoundary
      resetKeys={[props.points, props.clusters]}
      fallback={
        <div className='flex h-full w-full items-center justify-center p-4'>
          <ErrorAlert
            title='Failed to render cluster map'
            message='The cluster map could not be displayed.'
          />
        </div>
      }
    >
      <ScatterPlotInner {...props} />
    </ErrorBoundary>
  );
}

export default ScatterPlot;
