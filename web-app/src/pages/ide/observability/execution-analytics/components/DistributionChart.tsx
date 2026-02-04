import type { EChartsOption } from "echarts";
import { getInstanceByDom, init } from "echarts";
import { PieChart } from "lucide-react";
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
import { EXECUTION_TYPES, type ExecutionSummary } from "../types";

interface DistributionChartProps {
  summary: ExecutionSummary;
  isLoading: boolean;
}

export default function DistributionChart({ summary, isLoading }: DistributionChartProps) {
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

      const data = [
        {
          value: summary.semanticQueryCount,
          name: EXECUTION_TYPES.semantic_query.label,
          itemStyle: { color: EXECUTION_TYPES.semantic_query.chartColor }
        },
        {
          value: summary.omniQueryCount,
          name: EXECUTION_TYPES.omni_query.label,
          itemStyle: { color: EXECUTION_TYPES.omni_query.chartColor }
        },
        {
          value: summary.sqlGeneratedCount,
          name: EXECUTION_TYPES.sql_generated.label,
          itemStyle: { color: EXECUTION_TYPES.sql_generated.chartColor }
        },
        {
          value: summary.workflowCount,
          name: EXECUTION_TYPES.workflow.label,
          itemStyle: { color: EXECUTION_TYPES.workflow.chartColor }
        },
        {
          value: summary.agentToolCount,
          name: EXECUTION_TYPES.agent_tool.label,
          itemStyle: { color: EXECUTION_TYPES.agent_tool.chartColor }
        }
      ].filter((d) => d.value > 0);

      const options: EChartsOption = {
        tooltip: {},
        legend: {
          orient: "vertical",
          right: 10,
          top: "center",
          textStyle: {
            fontSize: 11
          },
          formatter: (name: string) => {
            const type = Object.values(EXECUTION_TYPES).find((t) => t.label === name);
            const icon = type?.category === "verified" ? "✓" : "✦";
            return `${icon} ${name}`;
          }
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
              borderWidth: 2
            },
            label: {
              show: false
            },
            emphasis: {
              label: {
                show: true,
                fontSize: 12,
                fontWeight: "bold"
              }
            },
            labelLine: {
              show: false
            },
            data
          }
        ]
      };
      chart?.setOption(options, true);
      chart?.resize();
    }
  }, [summary]);

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
        <div className='flex items-center justify-between'>
          <div className='flex items-center gap-2'>
            <PieChart className='h-5 w-5 text-primary' />
            <CardTitle>Type Breakdown</CardTitle>
          </div>
          <div className='flex items-center gap-3 text-xs'>
            <span className='flex items-center gap-1'>✦ Verified</span>
            <span className='flex items-center gap-1'>✓ Generated</span>
          </div>
        </div>
        <CardDescription>Distribution by execution type</CardDescription>
      </CardHeader>
      <CardContent>
        <div ref={chartRef} className='h-[220px] w-full' />
      </CardContent>
    </Card>
  );
}
