import { formatDistanceToNow } from "date-fns";
import { X } from "lucide-react";
import { Badge } from "@/components/ui/shadcn/badge";
import { Button } from "@/components/ui/shadcn/button";
import type { ClusterMapPoint, ClusterSummary } from "@/services/api/traces";

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

export default function QuestionDetailPanel({ point, cluster, onClose }: QuestionDetailPanelProps) {
  const status = getPointStatus(point);

  return (
    <div className='flex h-full flex-col border-l bg-background'>
      {/* Header */}
      <div className='flex items-center justify-between border-b p-4'>
        <div className='flex items-center gap-2'>
          <div
            className='h-3 w-3 rounded-full'
            style={{ backgroundColor: cluster?.color || "#9ca3af" }}
          />
          <span className='font-medium text-sm'>{cluster?.intentName || "Outlier"}</span>
        </div>
        <Button variant='ghost' size='icon' className='h-8 w-8' onClick={onClose}>
          <X className='h-4 w-4' />
        </Button>
      </div>

      {/* Content */}
      <div className='flex-1 space-y-4 overflow-auto p-4'>
        {/* Status & Metadata */}
        <div className='flex flex-wrap items-center gap-2'>
          <Badge variant='secondary' className={getStatusBadgeClass(status)}>
            {getStatusLabel(status)}
          </Badge>
          <span className='text-muted-foreground text-xs'>
            {formatDistanceToNow(new Date(point.timestamp), {
              addSuffix: true
            })}
          </span>
          {point.durationMs && (
            <span className='text-muted-foreground text-xs'>
              • {(point.durationMs / 1000).toFixed(2)}s
            </span>
          )}
          {point.confidence && (
            <span className='text-muted-foreground text-xs'>
              • {(point.confidence * 100).toFixed(0)}% confidence
            </span>
          )}
        </div>

        {/* Question */}
        <div>
          <h4 className='mb-2 font-medium text-muted-foreground text-xs uppercase tracking-wider'>
            Question
          </h4>
          <p className='rounded-lg bg-muted/50 p-3 text-sm'>{point.question}</p>
        </div>

        {/* Cluster Info */}
        {cluster && (
          <div>
            <h4 className='mb-2 font-medium text-muted-foreground text-xs uppercase tracking-wider'>
              Cluster Details
            </h4>
            <div className='space-y-2 rounded-lg bg-muted/50 p-3'>
              <div>
                <span className='text-muted-foreground text-xs'>Name:</span>
                <p className='font-medium text-sm'>{cluster.intentName}</p>
              </div>
              {cluster.description && (
                <div>
                  <span className='text-muted-foreground text-xs'>Description:</span>
                  <p className='text-sm'>{cluster.description}</p>
                </div>
              )}
              <div>
                <span className='text-muted-foreground text-xs'>Total queries:</span>
                <p className='font-medium text-sm'>{cluster.count}</p>
              </div>
            </div>
          </div>
        )}

        {/* Sample Questions from Cluster */}
        {cluster?.sampleQuestions && cluster.sampleQuestions.length > 0 && (
          <div>
            <h4 className='mb-2 font-medium text-muted-foreground text-xs uppercase tracking-wider'>
              Similar Questions
            </h4>
            <div className='space-y-2'>
              {cluster.sampleQuestions.slice(0, 3).map((q, i) => (
                <div key={i} className='rounded-lg bg-muted/50 p-2 text-muted-foreground text-sm'>
                  {q}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Trace Link */}
        <div className='pt-2'>
          <Button variant='outline' size='sm' className='w-full' asChild>
            <a href={`traces/${point.traceId}`} target='_blank' rel='noopener noreferrer'>
              View Full Trace
            </a>
          </Button>
        </div>
      </div>
    </div>
  );
}
