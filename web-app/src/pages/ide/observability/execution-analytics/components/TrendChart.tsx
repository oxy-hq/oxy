import type { EChartsOption } from "echarts";
import { getInstanceByDom, init } from "echarts";
import { TrendingUp } from "lucide-react";
import { useCallback, useEffect, useRef } from "react";
import { useResizeDetector } from "react-resize-detector";
import theme from "@/components/Echarts/theme.json";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle
} from "@/components/ui/shadcn/card";
import { useExecutionTimeSeries } from "@/hooks/api/useExecutionAnalytics";

interface TrendChartProps {
  projectId: string | undefined;
  days: number;
}

export default function TrendChart({ projectId, days }: TrendChartProps) {
  const { data: timeSeries = [], isLoading } = useExecutionTimeSeries(projectId, { days });
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

      const timestamps = timeSeries.map((item) => {
        const date = new Date(item.timestamp);
        return date.toLocaleDateString("en-US", {
          month: "short",
          day: "numeric"
        });
      });

      const options: EChartsOption = {
        tooltip: {},
        legend: {
          data: ["Verified", "Generated"],
          bottom: 0,
          textStyle: {
            fontSize: 11
          }
        },
        grid: {
          left: "3%",
          right: "4%",
          bottom: "15%",
          top: "10%",
          containLabel: true
        },
        xAxis: {
          type: "category",
          data: timestamps,
          axisLabel: {
            fontSize: 10,
            rotate: 0
          }
        },
        yAxis: {
          type: "value",
          axisLabel: {
            fontSize: 10
          }
        },
        series: [
          {
            name: "Verified",
            type: "bar",
            stack: "total",
            emphasis: {
              focus: "series"
            },
            data: timeSeries.map((item) => item.verifiedCount),
            itemStyle: { color: "#10b981" } // emerald
          },
          {
            name: "Generated",
            type: "bar",
            stack: "total",
            emphasis: {
              focus: "series"
            },
            data: timeSeries.map((item) => item.generatedCount),
            itemStyle: { color: "#f97316" } // orange
          }
        ]
      };
      chart?.setOption(options, true);
      chart?.resize();
    }
  }, [timeSeries]);

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
    <Card>
      <CardHeader className='pb-2'>
        <div className='flex items-center gap-2'>
          <TrendingUp className='h-5 w-5 text-primary' />
          <CardTitle>Verified vs Generated</CardTitle>
        </div>
        <CardDescription>Execution breakdown over time</CardDescription>
      </CardHeader>
      <CardContent>
        <div ref={chartRef} className='h-[220px] w-full' />
      </CardContent>
    </Card>
  );
}
