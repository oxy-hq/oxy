import { Echarts } from "@/components/Echarts";
import useChart from "@/hooks/api/useChart";
import useTheme from "@/stores/useTheme";
import { ChartConfig } from "@/types/chart";
import { EChartsOption } from "echarts";
import ChartError from "./Error";
import ChartLoading from "./Loading";
import { useAutoAnimate } from "@formkit/auto-animate/react";
import { ErrorBoundary } from "react-error-boundary";

type ChartProps = {
  data: string | undefined;
  isPending: boolean;
  error: Error | null;
  refetch: () => void;
};

type Props = {
  chart_src: string;
};

const Chart = (props: ChartProps) => {
  const { theme } = useTheme();
  const { data, isPending, error, refetch } = props;

  if (isPending) {
    return <ChartLoading />;
  }

  if (error) {
    return (
      <ChartError
        title="Failed to load chart"
        description={
          error?.message ??
          "There was an error loading the chart data. Please try again."
        }
        refetch={refetch}
      />
    );
  }

  let config: ChartConfig;
  try {
    config = JSON.parse(data || "{}") as ChartConfig;
  } catch {
    return (
      <ChartError
        title="Invalid chart data"
        description="The chart data format is invalid and cannot be displayed."
        refetch={refetch}
      />
    );
  }

  const isDarkMode = theme === "dark";

  const options: EChartsOption = {
    darkMode: isDarkMode,
    tooltip: {},
    xAxis: config.xAxis
      ? {
          type:
            (config.xAxis?.type as "category" | "value" | "time" | "log") ||
            "category",
          name: config.xAxis?.name,
          nameTextStyle: {
            color: isDarkMode ? "oklch(0.708 0 0)" : "oklch(0.556 0 0)",
            padding: [15, 0, 0, 0],
          },
          nameLocation: "middle",
          data: (config.xAxis?.data || []).map((d: string | number | Date) =>
            d instanceof Date ? d.toISOString() : d,
          ),
        }
      : undefined,
    yAxis: config.yAxis
      ? {
          type:
            (config.yAxis?.type as "category" | "value" | "time" | "log") ||
            "category",
          name: config.yAxis?.name,
          nameTextStyle: {
            color: isDarkMode ? "oklch(0.708 0 0)" : "oklch(0.556 0 0)",
          },
          data: (config.yAxis?.data || []).map((d: string | number | Date) =>
            d instanceof Date ? d.toISOString() : d,
          ),
        }
      : undefined,
    series: config.series.map((s) => ({
      name: s.name,
      type: s.type,
      data:
        s.data?.map((d) =>
          d && typeof d === "object" && "value" in d
            ? { name: d.name, value: d.value }
            : d,
        ) || [],
    })),
  };

  return (
    <Echarts options={options} isLoading={isPending} title={config.title} />
  );
};

const ChartContainer = (props: Props) => {
  const [parent] = useAutoAnimate({
    duration: 300,
  });
  const encodedPath = encodeURIComponent(props.chart_src);
  const { data, isPending, error, refetch } = useChart(encodedPath);

  return (
    <div ref={parent}>
      <ErrorBoundary
        fallback={
          <ChartError
            title="Chart error"
            description="An unexpected error occurred while rendering the chart."
            refetch={refetch}
          />
        }
      >
        <Chart
          data={data}
          isPending={isPending}
          error={error}
          refetch={refetch}
        />
      </ErrorBoundary>
    </div>
  );
};

export default ChartContainer;
