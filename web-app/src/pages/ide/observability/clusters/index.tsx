import { useState, useMemo, useCallback } from "react";
import { Button } from "@/components/ui/shadcn/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/shadcn/tooltip";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/shadcn/resizable";
import { Loader2, List, Clock, ScatterChart, LayoutGrid } from "lucide-react";
import useClusterMap from "@/hooks/api/traces/useClusterMap";
import type { ClusterMapPoint } from "@/services/api/traces";
import EmptyState from "@/components/ui/EmptyState";
import {
  ClusterList,
  DetailPanel,
  ScatterPlot,
  WaffleChart,
  SourceFilter,
} from "./components";

type ViewMode = "scatter" | "waffle";

const DAYS_OPTIONS = [
  { value: "7", label: "Last 7 days" },
  { value: "14", label: "Last 14 days" },
  { value: "30", label: "Last 30 days" },
  { value: "90", label: "Last 90 days" },
];

const LIMIT_OPTIONS = [
  { value: "100", label: "100 points" },
  { value: "250", label: "250 points" },
  { value: "500", label: "500 points" },
  { value: "1000", label: "1000 points" },
];

export default function ClusterMapPage() {
  const [days, setDays] = useState(30);
  const [limit, setLimit] = useState(100);
  const [showClusterList, setShowClusterList] = useState(true);
  const [hiddenClusters, setHiddenClusters] = useState<Set<number>>(new Set());
  const [selectedCluster, setSelectedCluster] = useState<number | null>(null);
  const [selectedPoint, setSelectedPoint] = useState<ClusterMapPoint | null>(
    null,
  );
  const [viewMode, setViewMode] = useState<ViewMode>("waffle");
  const [source, setSource] = useState<string | undefined>(undefined);

  const { data, isLoading, error } = useClusterMap(limit, days, true, source);

  const points = data?.points;
  const filteredPoints = useMemo(() => {
    if (!points) return [];
    return points.filter((p) => {
      if (hiddenClusters.has(p.clusterId)) return false;
      if (selectedCluster !== null && p.clusterId !== selectedCluster)
        return false;
      return true;
    });
  }, [points, hiddenClusters, selectedCluster]);

  const clusterColorMap = useMemo(() => {
    const map = new Map<number, string>();
    data?.clusters?.forEach((c) => {
      map.set(c.clusterId, c.color);
    });
    return map;
  }, [data?.clusters]);

  const getPointColor = useCallback(
    (point: ClusterMapPoint) =>
      clusterColorMap.get(point.clusterId) || "#9ca3af",
    [clusterColorMap],
  );

  const toggleClusterVisibility = useCallback((clusterId: number) => {
    setHiddenClusters((prev) => {
      const next = new Set(prev);
      if (next.has(clusterId)) {
        next.delete(clusterId);
      } else {
        next.add(clusterId);
      }
      return next;
    });
  }, []);

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full text-destructive">
        Failed to load cluster map data
      </div>
    );
  }

  const clusterCount =
    data?.clusters.filter((c) => c.clusterId !== -1).length || 0;
  const hasData = data && data.totalPoints > 0;

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex flex-col min-h-0">
        <ActionsSection
          days={days}
          limit={limit}
          showClusterList={showClusterList}
          viewMode={viewMode}
          source={source}
          totalPoints={data?.totalPoints || 0}
          clusterCount={clusterCount}
          outlierCount={data?.outlierCount || 0}
          onDaysChange={setDays}
          onLimitChange={setLimit}
          onToggleClusterList={() => setShowClusterList(!showClusterList)}
          onViewModeChange={setViewMode}
          onSourceChange={setSource}
        />

        {!hasData ? (
          <div className="flex-1 flex items-center justify-center">
            <EmptyState
              title="No traces found"
              description="No trace data available for the selected time range and filters"
            />
          </div>
        ) : (
          <ResizablePanelGroup direction="horizontal" className="flex-1">
            {showClusterList && data?.clusters && (
              <>
                <ResizablePanel
                  defaultSize={20}
                  minSize={15}
                  maxSize={40}
                  className="min-w-0"
                >
                  <ClusterList
                    clusters={data.clusters}
                    hiddenClusters={hiddenClusters}
                    selectedCluster={selectedCluster}
                    onToggleVisibility={toggleClusterVisibility}
                    onSelectCluster={setSelectedCluster}
                  />
                </ResizablePanel>
                <ResizableHandle withHandle />
              </>
            )}

            <ResizablePanel defaultSize={80} minSize={40} className="min-w-0">
              <div className="h-full flex overflow-hidden">
                <div className="flex-1 relative overflow-hidden bg-muted/20">
                  {viewMode === "scatter" ? (
                    <ScatterPlot
                      points={filteredPoints}
                      getPointColor={getPointColor}
                      clusters={data?.clusters || []}
                      onPointClick={setSelectedPoint}
                      selectedPoint={selectedPoint}
                    />
                  ) : (
                    <WaffleChart
                      points={filteredPoints}
                      clusters={data?.clusters || []}
                      onPointClick={setSelectedPoint}
                      selectedPoint={selectedPoint}
                    />
                  )}
                </div>

                {selectedPoint && (
                  <div className="w-80 shrink-0">
                    <DetailPanel
                      point={selectedPoint}
                      cluster={data?.clusters.find(
                        (c) => c.clusterId === selectedPoint.clusterId,
                      )}
                      onClose={() => setSelectedPoint(null)}
                    />
                  </div>
                )}
              </div>
            </ResizablePanel>
          </ResizablePanelGroup>
        )}
      </div>
    </div>
  );
}

interface ActionsSectionProps {
  days: number;
  limit: number;
  showClusterList: boolean;
  viewMode: ViewMode;
  source: string | undefined;
  totalPoints: number;
  clusterCount: number;
  outlierCount: number;
  onDaysChange: (days: number) => void;
  onLimitChange: (limit: number) => void;
  onToggleClusterList: () => void;
  onViewModeChange: (mode: ViewMode) => void;
  onSourceChange: (source: string | undefined) => void;
}

function ActionsSection({
  days,
  limit,
  showClusterList,
  viewMode,
  source,
  totalPoints,
  clusterCount,
  outlierCount,
  onDaysChange,
  onLimitChange,
  onToggleClusterList,
  onViewModeChange,
  onSourceChange,
}: ActionsSectionProps) {
  return (
    <div className="p-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4 text-sm text-muted-foreground">
          <span>Total: {totalPoints} traces</span>
          <span>Clusters: {clusterCount}</span>
          <span>Outliers: {outlierCount}</span>
        </div>

        <div className="flex items-center gap-2">
          <Select
            value={days.toString()}
            onValueChange={(v) => onDaysChange(parseInt(v))}
          >
            <SelectTrigger>
              <Clock className="h-4 w-4 mr-2" />
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {DAYS_OPTIONS.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  {opt.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select
            value={limit.toString()}
            onValueChange={(v) => onLimitChange(parseInt(v))}
          >
            <SelectTrigger className="w-32">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {LIMIT_OPTIONS.map((opt) => (
                <SelectItem key={opt.value} value={opt.value}>
                  {opt.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <SourceFilter onSelect={onSourceChange} selectedSource={source} />

          <div className="flex items-center rounded-md border border-input">
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant={viewMode === "waffle" ? "secondary" : "ghost"}
                  size="sm"
                  className="rounded-r-none border-0"
                  onClick={() => onViewModeChange("waffle")}
                >
                  <LayoutGrid className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="bottom" className="max-w-xs">
                <p className="font-medium">Grid View</p>
                <p className="text-xs opacity-80">
                  Display clusters as grouped grids showing success/failure
                  status
                </p>
              </TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  variant={viewMode === "scatter" ? "secondary" : "ghost"}
                  size="sm"
                  className="rounded-l-none border-0"
                  onClick={() => onViewModeChange("scatter")}
                >
                  <ScatterChart className="h-4 w-4" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="bottom" className="max-w-xs">
                <p className="font-medium">Semantic Cluster Map</p>
                <p className="text-xs opacity-80">
                  Visualize queries by semantic similarity. Points closer
                  together have similar meaning.
                </p>
              </TooltipContent>
            </Tooltip>
          </div>

          <Button variant="outline" size="sm" onClick={onToggleClusterList}>
            <List className="h-4 w-4 mr-2" />
            {showClusterList ? "Hide" : "Show"} clusters list
          </Button>
        </div>
      </div>
    </div>
  );
}
