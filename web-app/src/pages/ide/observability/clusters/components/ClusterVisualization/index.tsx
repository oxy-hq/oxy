import { useState, useMemo, useCallback } from "react";
import { Button } from "@/components/ui/shadcn/button";
import { Badge } from "@/components/ui/shadcn/badge";
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
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Eye, EyeOff, List, ScatterChart, LayoutGrid, Map } from "lucide-react";
import type { ClusterSummary, ClusterMapPoint } from "@/services/api/traces";
import QuestionDetailPanel from "./QuestionDetailPanel";
import ScatterPlot from "./ScatterPlot";
import WaffleChart from "./WaffleChart";

type ViewMode = "scatter" | "waffle";

interface ClusterVisualizationProps {
  clusters: ClusterSummary[];
  points: ClusterMapPoint[];
  clusterColorMap: Map<number, string>;
}

export default function ClusterVisualization({
  clusters,
  points,
  clusterColorMap,
}: ClusterVisualizationProps) {
  const [viewMode, setViewMode] = useState<ViewMode>("waffle");
  const [showClusterList, setShowClusterList] = useState(true);
  const [hiddenClusters, setHiddenClusters] = useState<Set<number>>(new Set());
  const [selectedCluster, setSelectedCluster] = useState<number | null>(null);
  const [internalSelectedPoint, setInternalSelectedPoint] =
    useState<ClusterMapPoint | null>(null);

  const filteredPoints = useMemo(() => {
    return points.filter((p) => {
      if (hiddenClusters.has(p.clusterId)) return false;
      if (selectedCluster !== null && p.clusterId !== selectedCluster)
        return false;
      return true;
    });
  }, [points, hiddenClusters, selectedCluster]);

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

  const handlePointClick = useCallback((point: ClusterMapPoint) => {
    setInternalSelectedPoint(point);
  }, []);

  const selectedPointCluster = useMemo(() => {
    if (!internalSelectedPoint) return undefined;
    return clusters.find(
      (c) => c.clusterId === internalSelectedPoint.clusterId,
    );
  }, [internalSelectedPoint, clusters]);

  return (
    <Card className="h-[800px] flex flex-col">
      <CardHeader className="pb-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Map className="h-5 w-5 text-primary" />
            <CardTitle>Cluster Visualization</CardTitle>
          </div>
          <div className="flex items-center gap-2">
            {/* View Mode Toggle */}
            <div className="flex items-center rounded-md border border-input">
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant={viewMode === "waffle" ? "secondary" : "ghost"}
                    size="sm"
                    className="rounded-r-none border-0 h-7"
                    onClick={() => setViewMode("waffle")}
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
                    className="rounded-l-none border-0 h-7"
                    onClick={() => setViewMode("scatter")}
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

            {/* Toggle Cluster List */}
            <Button
              variant="outline"
              size="sm"
              className="h-7"
              onClick={() => setShowClusterList(!showClusterList)}
            >
              <List className="h-4 w-4 mr-1" />
              {showClusterList ? "Hide" : "Show"} list
            </Button>
          </div>
        </div>
        <CardDescription>
          Points represent user queries, positioned by semantic similarity
        </CardDescription>
      </CardHeader>
      <CardContent className="pt-0 flex-1 min-h-0">
        <ResizablePanelGroup
          direction="horizontal"
          className="h-full rounded-lg border"
        >
          {showClusterList && (
            <>
              <ResizablePanel
                defaultSize={25}
                minSize={20}
                maxSize={40}
                className="min-w-0"
              >
                <ClusterList
                  clusters={clusters}
                  hiddenClusters={hiddenClusters}
                  selectedCluster={selectedCluster}
                  onToggleVisibility={toggleClusterVisibility}
                  onSelectCluster={setSelectedCluster}
                />
              </ResizablePanel>
              <ResizableHandle withHandle />
            </>
          )}

          <ResizablePanel
            defaultSize={internalSelectedPoint ? 50 : 75}
            minSize={30}
            className="min-w-0"
          >
            <div className="h-full relative overflow-hidden bg-muted/20">
              {viewMode === "scatter" ? (
                <ScatterPlot
                  points={filteredPoints}
                  getPointColor={getPointColor}
                  clusters={clusters}
                  onPointClick={handlePointClick}
                  selectedPoint={internalSelectedPoint}
                />
              ) : (
                <WaffleChart
                  points={filteredPoints}
                  clusters={clusters}
                  onPointClick={handlePointClick}
                  selectedPoint={internalSelectedPoint}
                />
              )}
            </div>
          </ResizablePanel>

          {/* Point Detail Panel */}
          {internalSelectedPoint && (
            <>
              <ResizableHandle withHandle />
              <ResizablePanel
                defaultSize={25}
                minSize={20}
                maxSize={40}
                className="min-w-0"
              >
                <QuestionDetailPanel
                  point={internalSelectedPoint}
                  cluster={selectedPointCluster}
                  onClose={() => setInternalSelectedPoint(null)}
                />
              </ResizablePanel>
            </>
          )}
        </ResizablePanelGroup>
      </CardContent>
    </Card>
  );
}

// Internal ClusterList component
interface ClusterListProps {
  clusters: ClusterSummary[];
  hiddenClusters: Set<number>;
  selectedCluster: number | null;
  onToggleVisibility: (clusterId: number) => void;
  onSelectCluster: (clusterId: number | null) => void;
}

function ClusterList({
  clusters,
  hiddenClusters,
  selectedCluster,
  onToggleVisibility,
  onSelectCluster,
}: ClusterListProps) {
  return (
    <div className="h-full overflow-y-auto p-3 space-y-1.5 customScrollbar bg-background">
      <div className="flex items-center justify-between mb-2">
        <h3 className="font-medium text-sm">Clusters</h3>
        {selectedCluster !== null && (
          <Button
            variant="ghost"
            size="sm"
            className="h-6 text-xs"
            onClick={() => onSelectCluster(null)}
          >
            Show All
          </Button>
        )}
      </div>
      {clusters.map((cluster) => (
        <ClusterListItem
          key={cluster.clusterId}
          cluster={cluster}
          isHidden={hiddenClusters.has(cluster.clusterId)}
          isSelected={selectedCluster === cluster.clusterId}
          onToggleVisibility={() => onToggleVisibility(cluster.clusterId)}
          onSelect={() =>
            onSelectCluster(
              selectedCluster === cluster.clusterId ? null : cluster.clusterId,
            )
          }
        />
      ))}
    </div>
  );
}

interface ClusterListItemProps {
  cluster: ClusterSummary;
  isHidden: boolean;
  isSelected: boolean;
  onToggleVisibility: () => void;
  onSelect: () => void;
}

function ClusterListItem({
  cluster,
  isHidden,
  isSelected,
  onToggleVisibility,
  onSelect,
}: ClusterListItemProps) {
  const containerClasses = `p-2 rounded-lg border cursor-pointer transition-colors ${
    isSelected ? "border-primary bg-primary/5" : "hover:bg-muted/50"
  } ${isHidden ? "opacity-50" : ""}`;

  return (
    <div className={containerClasses} onClick={onSelect}>
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0 flex-1">
          <div
            className="w-2.5 h-2.5 rounded-full shrink-0"
            style={{ backgroundColor: cluster.color }}
          />
          <span className="text-xs font-medium truncate">
            {cluster.intentName}
          </span>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <Badge variant="secondary" className="text-xs h-5 px-1.5">
            {cluster.count}
          </Badge>
          <Button
            variant="ghost"
            size="icon"
            className="h-5 w-5"
            onClick={(e) => {
              e.stopPropagation();
              onToggleVisibility();
            }}
          >
            {isHidden ? (
              <EyeOff className="h-3 w-3" />
            ) : (
              <Eye className="h-3 w-3" />
            )}
          </Button>
        </div>
      </div>
    </div>
  );
}
