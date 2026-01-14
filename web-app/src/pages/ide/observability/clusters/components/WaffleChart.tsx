import { useMemo, useRef } from "react";
import type { ClusterMapPoint, ClusterSummary } from "@/services/api/traces";
import { cn } from "@/libs/shadcn/utils";

interface WaffleChartProps {
  points: ClusterMapPoint[];
  clusters: ClusterSummary[];
  onPointClick: (point: ClusterMapPoint) => void;
  selectedPoint: ClusterMapPoint | null;
}

interface ClusterGroup {
  cluster: ClusterSummary;
  points: ClusterMapPoint[];
  answeredCount: number;
  failedCount: number;
  noDataCount: number;
}

type PointStatus = "answered" | "failed" | "no-data";

function getPointStatus(point: ClusterMapPoint): PointStatus {
  // Determine status based on output and duration
  if (point.output && point.output.trim().length > 0) {
    return "answered";
  }
  if (point.durationMs && point.durationMs > 0 && !point.output) {
    return "failed";
  }
  return "no-data";
}

export function WaffleChart({
  points,
  clusters,
  onPointClick,
  selectedPoint,
}: WaffleChartProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  const clusterGroups = useMemo(() => {
    const grouped = new Map<number, ClusterMapPoint[]>();

    points.forEach((point) => {
      const existing = grouped.get(point.clusterId) || [];
      existing.push(point);
      grouped.set(point.clusterId, existing);
    });

    const groups: ClusterGroup[] = [];

    // Sort by cluster count (outliers first if they exist, then by count)
    const sortedEntries = Array.from(grouped.entries()).sort(
      ([idA, pointsA], [idB, pointsB]) => {
        // Outliers (clusterId === -1) should come first
        if (idA === -1) return -1;
        if (idB === -1) return 1;
        return pointsB.length - pointsA.length;
      },
    );

    sortedEntries.forEach(([clusterId, clusterPoints]) => {
      const cluster = clusters.find((c) => c.clusterId === clusterId);
      if (!cluster) return;

      let answeredCount = 0;
      let failedCount = 0;
      let noDataCount = 0;

      clusterPoints.forEach((point) => {
        const status = getPointStatus(point);
        if (status === "answered") answeredCount++;
        else if (status === "failed") failedCount++;
        else noDataCount++;
      });

      groups.push({
        cluster,
        points: clusterPoints,
        answeredCount,
        failedCount,
        noDataCount,
      });
    });

    return groups;
  }, [points, clusters]);

  return (
    <div className="h-full flex flex-col">
      <div ref={containerRef} className="flex-1 overflow-auto p-4">
        <div className="flex flex-wrap gap-6 content-start">
          {clusterGroups.map((group) => (
            <ClusterBox
              key={group.cluster.clusterId}
              group={group}
              onPointClick={onPointClick}
              selectedPoint={selectedPoint}
            />
          ))}
        </div>
      </div>

      {/* Legend */}
      <div className="flex items-center justify-end gap-6 px-4 py-3 border-t border-border text-xs text-muted-foreground">
        <div className="flex items-center gap-2">
          <div className="w-3 h-3 rounded-sm bg-blue-500" />
          <span>answered</span>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-3 h-3 rounded-sm bg-pink-400" />
          <span>failed to answer</span>
        </div>
      </div>
    </div>
  );
}

interface ClusterBoxProps {
  group: ClusterGroup;
  onPointClick: (point: ClusterMapPoint) => void;
  selectedPoint: ClusterMapPoint | null;
}

function ClusterBox({ group, onPointClick, selectedPoint }: ClusterBoxProps) {
  const { cluster, points, failedCount } = group;

  // Calculate grid dimensions - aim for roughly square grid
  const totalPoints = points.length;
  const cols = Math.ceil(Math.sqrt(totalPoints * 1.5)); // Slightly wider than tall
  const rows = Math.ceil(totalPoints / cols);

  return (
    <div className="flex flex-col gap-2">
      {/* Header */}
      <div className="flex items-center gap-2 text-xs">
        <span
          className="font-medium truncate max-w-[120px]"
          title={cluster.intentName}
        >
          {cluster.intentName}
        </span>
        <span className="text-blue-500">{points.length}</span>
        {failedCount > 0 && (
          <span className="text-pink-400">{failedCount}</span>
        )}
      </div>

      {/* Waffle grid */}
      <div
        className="grid gap-[2px]"
        style={{
          gridTemplateColumns: `repeat(${cols}, 10px)`,
          gridTemplateRows: `repeat(${rows}, 10px)`,
        }}
      >
        {points.map((point) => (
          <WaffleCell
            key={point.traceId}
            point={point}
            clusterColor={cluster.color}
            isSelected={selectedPoint?.traceId === point.traceId}
            onClick={() => onPointClick(point)}
          />
        ))}
      </div>
    </div>
  );
}

interface WaffleCellProps {
  point: ClusterMapPoint;
  clusterColor: string;
  isSelected: boolean;
  onClick: () => void;
}

function WaffleCell({
  point,
  clusterColor,
  isSelected,
  onClick,
}: WaffleCellProps) {
  const status = getPointStatus(point);

  const getBackgroundColor = () => {
    switch (status) {
      case "answered":
        return clusterColor;
      case "failed":
        return "#f472b6"; // pink-400
      case "no-data":
        return clusterColor; // default to cluster color when no data
    }
  };

  return (
    <button
      className={cn(
        "w-[10px] h-[10px] rounded-sm transition-all cursor-pointer",
        "hover:scale-125 hover:z-10 hover:ring-2 hover:ring-white hover:ring-offset-1",
        isSelected && "ring-2 ring-white ring-offset-1 scale-125 z-10",
      )}
      style={{ backgroundColor: getBackgroundColor() }}
      onClick={onClick}
      title={point.question}
    />
  );
}
