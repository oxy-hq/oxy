import { useSearchParams } from "react-router-dom";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { useMetricAnomalies } from "@/hooks/api/useMetricAnomalies";
import MetricTreeView from "../MetricTree";
import AnomaliesInbox from "./AnomaliesInbox";
import SemanticExplorerTab from "./SemanticExplorerTab";

const TAB_VALUES = ["explorer", "metric-tree", "anomalies"] as const;
type TabValue = (typeof TAB_VALUES)[number];

/** The Semantic Layer IDE tab: topic/view Explorer + Metric Tree + Anomalies inbox. */
export default function SemanticLayerPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const { data: newAnomalies } = useMetricAnomalies("new");
  const newCount = newAnomalies?.length ?? 0;
  const raw = searchParams.get("view");
  // "insights" was the old URL value — redirect existing links gracefully.
  const normalised = raw === "insights" ? "anomalies" : raw;
  const active: TabValue = TAB_VALUES.includes(normalised as TabValue)
    ? (normalised as TabValue)
    : "explorer";

  return (
    <div className='flex h-full min-h-0 flex-1 flex-col overflow-hidden'>
      <Tabs
        value={active}
        onValueChange={(v) => setSearchParams({ view: v }, { replace: true })}
        className='flex min-h-0 flex-1 flex-col'
      >
        <div className='border-border border-b px-2 py-2'>
          <TabsList className='w-fit'>
            <TabsTrigger value='explorer'>Explorer</TabsTrigger>
            <TabsTrigger value='metric-tree'>Metric Tree</TabsTrigger>
            <TabsTrigger value='anomalies' className='gap-1.5'>
              Anomalies
              {newCount > 0 && (
                <span className='rounded-full bg-destructive px-1.5 py-0.5 font-medium text-[10px] text-destructive-foreground leading-none'>
                  {newCount}
                </span>
              )}
            </TabsTrigger>
          </TabsList>
        </div>
        <TabsContent value='explorer' className='min-h-0 flex-1'>
          <SemanticExplorerTab />
        </TabsContent>
        <TabsContent value='metric-tree' className='min-h-0 flex-1'>
          <MetricTreeView />
        </TabsContent>
        <TabsContent value='anomalies' className='min-h-0 flex-1'>
          <AnomaliesInbox />
        </TabsContent>
      </Tabs>
    </div>
  );
}
