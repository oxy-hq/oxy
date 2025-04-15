import Chart from "@/components/ui/Chart";
import useChart from "@/hooks/api/useChart";
import { TopLevelSpec } from "vega-lite";

type Props = {
  file_path: string;
};

export default function ChartContainer(props: Props) {
  const encodedPath = encodeURIComponent(props.file_path);
  const { data, isLoading } = useChart(encodedPath);

  if (isLoading) return <div>Loading...</div>;

  const spec = JSON.parse(data || "{}");

  if (!spec) return null;

  return (
    <div className="w-full h-[320px]">
      <Chart spec={spec as TopLevelSpec} />
    </div>
  );
}
