import { Button } from "@/components/ui/shadcn/button";
import { Badge } from "@/components/ui/shadcn/badge";
import { Eye, EyeOff } from "lucide-react";
import type { ClusterSummary } from "@/services/api/traces";

interface ClusterListProps {
  clusters: ClusterSummary[];
  hiddenClusters: Set<number>;
  selectedCluster: number | null;
  onToggleVisibility: (clusterId: number) => void;
  onSelectCluster: (clusterId: number | null) => void;
}

export function ClusterList({
  clusters,
  hiddenClusters,
  selectedCluster,
  onToggleVisibility,
  onSelectCluster,
}: ClusterListProps) {
  return (
    <div className="h-full border-r overflow-y-auto p-4 space-y-2 customScrollbar">
      <ClusterListHeader
        selectedCluster={selectedCluster}
        onClearSelection={() => onSelectCluster(null)}
      />
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

interface ClusterListHeaderProps {
  selectedCluster: number | null;
  onClearSelection: () => void;
}

function ClusterListHeader({
  selectedCluster,
  onClearSelection,
}: ClusterListHeaderProps) {
  return (
    <div className="flex items-center justify-between mb-4">
      <h3 className="font-semibold">Clusters</h3>
      {selectedCluster !== null && (
        <Button variant="ghost" size="sm" onClick={onClearSelection}>
          Show All
        </Button>
      )}
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
            className="w-3 h-3 rounded-full shrink-0"
            style={{ backgroundColor: cluster.color }}
          />
          <span className="text-sm font-medium truncate">
            {cluster.intentName}
          </span>
        </div>
        <div className="flex items-center gap-1 shrink-0">
          <Badge variant="secondary" className="text-xs">
            {cluster.count}
          </Badge>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
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
      <p className="text-xs text-muted-foreground mt-1 line-clamp-1">
        {cluster.description}
      </p>
    </div>
  );
}
