import { X } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { Badge } from "@/components/ui/shadcn/badge";
import type { ClusterMapPoint, ClusterSummary } from "@/services/api/traces";
import { formatDistanceToNow } from "date-fns";

interface QuestionDetailPanelProps {
  point: ClusterMapPoint;
  cluster?: ClusterSummary;
  onClose: () => void;
}

type PointStatus = "ok" | "error" | "unset";

function getPointStatus(point: ClusterMapPoint): PointStatus {
  // Use status from API if available
  if (point.status) {
    return point.status;
  }
  return "unset";
}

function getStatusBadgeClass(status: PointStatus): string {
  switch (status) {
    case "ok":
      return "bg-emerald-500/10 text-emerald-600";
    case "error":
      return "bg-rose-500/10 text-rose-600";
    default:
      return "bg-muted text-muted-foreground";
  }
}

function getStatusLabel(status: PointStatus): string {
  switch (status) {
    case "ok":
      return "Success";
    case "error":
      return "Error";
    default:
      return "Unknown";
  }
}

export default function QuestionDetailPanel({
  point,
  cluster,
  onClose,
}: QuestionDetailPanelProps) {
  const status = getPointStatus(point);

  return (
    <div className="h-full flex flex-col border-l bg-background">
      {/* Header */}
      <div className="flex items-center justify-between p-4 border-b">
        <div className="flex items-center gap-2">
          <div
            className="w-3 h-3 rounded-full"
            style={{ backgroundColor: cluster?.color || "#9ca3af" }}
          />
          <span className="font-medium text-sm">
            {cluster?.intentName || "Outlier"}
          </span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-8 w-8"
          onClick={onClose}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-4 space-y-4">
        {/* Status & Metadata */}
        <div className="flex items-center gap-2 flex-wrap">
          <Badge variant="secondary" className={getStatusBadgeClass(status)}>
            {getStatusLabel(status)}
          </Badge>
          <span className="text-xs text-muted-foreground">
            {formatDistanceToNow(new Date(point.timestamp), {
              addSuffix: true,
            })}
          </span>
          {point.durationMs && (
            <span className="text-xs text-muted-foreground">
              • {(point.durationMs / 1000).toFixed(2)}s
            </span>
          )}
          {point.confidence && (
            <span className="text-xs text-muted-foreground">
              • {(point.confidence * 100).toFixed(0)}% confidence
            </span>
          )}
        </div>

        {/* Question */}
        <div>
          <h4 className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-2">
            Question
          </h4>
          <p className="text-sm bg-muted/50 rounded-lg p-3">{point.question}</p>
        </div>

        {/* Cluster Info */}
        {cluster && (
          <div>
            <h4 className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-2">
              Cluster Details
            </h4>
            <div className="bg-muted/50 rounded-lg p-3 space-y-2">
              <div>
                <span className="text-xs text-muted-foreground">Name:</span>
                <p className="text-sm font-medium">{cluster.intentName}</p>
              </div>
              {cluster.description && (
                <div>
                  <span className="text-xs text-muted-foreground">
                    Description:
                  </span>
                  <p className="text-sm">{cluster.description}</p>
                </div>
              )}
              <div>
                <span className="text-xs text-muted-foreground">
                  Total queries:
                </span>
                <p className="text-sm font-medium">{cluster.count}</p>
              </div>
            </div>
          </div>
        )}

        {/* Sample Questions from Cluster */}
        {cluster?.sampleQuestions && cluster.sampleQuestions.length > 0 && (
          <div>
            <h4 className="text-xs font-medium text-muted-foreground uppercase tracking-wider mb-2">
              Similar Questions
            </h4>
            <div className="space-y-2">
              {cluster.sampleQuestions.slice(0, 3).map((q, i) => (
                <div
                  key={i}
                  className="text-sm bg-muted/50 rounded-lg p-2 text-muted-foreground"
                >
                  {q}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Trace Link */}
        <div className="pt-2">
          <Button variant="outline" size="sm" className="w-full" asChild>
            <a
              href={`traces/${point.traceId}`}
              target="_blank"
              rel="noopener noreferrer"
            >
              View Full Trace
            </a>
          </Button>
        </div>
      </div>
    </div>
  );
}
