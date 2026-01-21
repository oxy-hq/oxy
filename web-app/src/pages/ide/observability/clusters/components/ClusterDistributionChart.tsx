import { useCallback, useEffect, useRef, useMemo } from "react";
import type { EChartsOption } from "echarts";
import { init, getInstanceByDom } from "echarts";
import theme from "@/components/Echarts/theme.json";
import { useResizeDetector } from "react-resize-detector";
import type { ClusterSummary } from "@/services/api/traces";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { PieChart as PieChartIcon, Loader2 } from "lucide-react";

interface ClusterDistributionChartProps {
  clusters: ClusterSummary[];
  isLoading: boolean;
}

export default function ClusterDistributionChart({
  clusters,
  isLoading,
}: ClusterDistributionChartProps) {
  const chartRef = useRef<HTMLDivElement>(null);

  const onResize = useCallback(() => {
    if (chartRef.current) {
      const chart = getInstanceByDom(chartRef.current);
      chart?.resize();
    }
  }, []);

  useResizeDetector({
    targetRef: chartRef,
    onResize,
  });

  useEffect(() => {
    if (!chartRef.current) return;
    const chart = init(chartRef.current, theme);

    return () => {
      chart.dispose();
    };
  }, []);

  const chartData = useMemo(() => {
    // Get top clusters (limit to 5 for readability, combine rest into "Other")
    const sortedClusters = [...clusters]
      .filter((c) => c.clusterId !== -1)
      .sort((a, b) => b.count - a.count);

    const topClusters = sortedClusters.slice(0, 5);
    const otherClusters = sortedClusters.slice(5);
    const otherCount = otherClusters.reduce((sum, c) => sum + c.count, 0);

    const data = topClusters.map((c) => ({
      value: c.count,
      name: c.intentName,
      itemStyle: { color: c.color },
    }));

    // Add outliers if they exist
    const outlierCluster = clusters.find((c) => c.clusterId === -1);
    if (outlierCluster && outlierCluster.count > 0) {
      data.push({
        value: outlierCluster.count,
        name: "Outliers",
        itemStyle: { color: "#6b7280" },
      });
    }

    if (otherCount > 0) {
      data.push({
        value: otherCount,
        name: "Other",
        itemStyle: { color: "#9ca3af" },
      });
    }

    return data;
  }, [clusters]);

  useEffect(() => {
    if (chartRef.current) {
      const chart = getInstanceByDom(chartRef.current);

      const options: EChartsOption = {
        tooltip: {
          trigger: "item",
          formatter: "{b}: {c} ({d}%)",
        },
        legend: {
          orient: "vertical",
          right: 10,
          top: "center",
          textStyle: {
            fontSize: 11,
          },
          type: "scroll",
        },
        series: [
          {
            type: "pie",
            radius: ["40%", "70%"],
            center: ["35%", "50%"],
            avoidLabelOverlap: false,
            itemStyle: {
              borderRadius: 4,
              borderColor: "#fff",
              borderWidth: 2,
            },
            label: {
              show: false,
            },
            emphasis: {
              label: {
                show: true,
                fontSize: 12,
                fontWeight: "bold",
              },
            },
            labelLine: {
              show: false,
            },
            data: chartData,
          },
        ],
      };
      chart?.setOption(options, true);
      chart?.resize();
    }
  }, [chartData]);

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
    <Card className="h-full">
      <CardHeader className="pb-2">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <PieChartIcon className="h-5 w-5 text-primary" />
            <CardTitle>Cluster Distribution</CardTitle>
            {isLoading && (
              <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
            )}
          </div>
        </div>
        <CardDescription>Distribution of questions by cluster</CardDescription>
      </CardHeader>
      <CardContent className="pt-0">
        <div ref={chartRef} style={{ height: 260, width: "100%" }} />
      </CardContent>
    </Card>
  );
}
