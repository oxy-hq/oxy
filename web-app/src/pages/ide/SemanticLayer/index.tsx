import { useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import { useMetricAnomalies } from "@/hooks/api/useMetricAnomalies";
import MetricTreeView from "../MetricTree";
import AnomaliesInbox, { FIRST_PAGE, PAGE_SIZE } from "./AnomaliesInbox";
import { groupIntoEvents } from "./AnomaliesInbox/components/events";
import PreAggregationTab from "./PreAggregationTab";
import SemanticExplorerTab from "./SemanticExplorerTab";

// World Model graduated to its own first-class IDE sidebar surface
// (`/ide/world-model`), so it is no longer a tab here.
const TAB_VALUES = ["explorer", "metric-tree", "pre-aggregation", "anomalies"] as const;
type TabValue = (typeof TAB_VALUES)[number];

/** The Semantic Model IDE tab: Explorer + Metric Tree + Pre-aggregation + Anomalies inbox. */
export default function SemanticLayerPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  // Ask for the inbox's own first page, not a page of the badge's own size:
  // same query key, so the two share one cache entry and one request, and a
  // workspace whose anomalies fit on that page skips the server's COUNT
  // entirely. A `limit: 1` "just the count" page would defeat that
  // short-circuit and force the COUNT on every mount.
  const { data: newAnomalies } = useMetricAnomalies("new", undefined, FIRST_PAGE);
  // `total` counts events, not rows, so the badge agrees with the event-grouped
  // table the inbox renders (a multi-bucket slide is one anomaly, not N) — and,
  // unlike grouping the returned rows, it counts every page.
  //
  // When it is absent the server served the page but its count query failed.
  // Reading that as zero would hide the badge on a workspace full of new
  // anomalies, so fall back to counting the page we did get and mark it as a
  // floor.
  const total = newAnomalies?.total;
  const pageCount = useMemo(
    () => groupIntoEvents(newAnomalies?.anomalies ?? []).length,
    [newAnomalies]
  );
  const newCount = total ?? pageCount;
  // Against the page size the server *served*, like every equivalent inside the
  // inbox: `limit` is clamped to 1..=500, and a page count can never reach a
  // `PAGE_SIZE` the server declined to serve — which would drop the `+` and
  // present a floor as an exact count.
  const countIsFloor = total === undefined && pageCount >= (newAnomalies?.limit || PAGE_SIZE);

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
            <TabsTrigger value='pre-aggregation'>Pre-aggregation</TabsTrigger>
            <TabsTrigger value='anomalies' className='gap-1.5'>
              Anomalies
              {newCount > 0 && (
                <span className='rounded-full bg-destructive px-1.5 py-0.5 font-medium text-[10px] text-destructive-foreground leading-none'>
                  {newCount}
                  {/* A page-derived count is a floor, not a total. */}
                  {countIsFloor && "+"}
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
        <TabsContent value='pre-aggregation' className='min-h-0 flex-1'>
          <PreAggregationTab />
        </TabsContent>
        <TabsContent value='anomalies' className='min-h-0 flex-1'>
          <AnomaliesInbox />
        </TabsContent>
      </Tabs>
    </div>
  );
}
