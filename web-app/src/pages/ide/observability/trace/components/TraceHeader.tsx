import { Link } from "react-router-dom";
import { ArrowLeft, Clock, Layers } from "lucide-react";
import { Button } from "@/components/ui/shadcn/button";
import { formatDuration, formatTimeAgo } from "../../utils/index";
import useCurrentProject from "@/stores/useCurrentProject";
import ROUTES from "@/libs/utils/routes";

interface TraceHeaderProps {
  traceId: string;
  totalDurationMs: number;
  spansCount: number;
  startTime: string;
}

export function TraceHeader({
  traceId,
  totalDurationMs,
  spansCount,
  startTime,
}: TraceHeaderProps) {
  const { project } = useCurrentProject();
  return (
    <div className="border-b px-4 py-3 flex items-center justify-between">
      <div className="flex items-center gap-3">
        <Button variant="ghost" size="icon" className="h-8 w-8" asChild>
          <Link to={ROUTES.PROJECT(project?.id || "").IDE.OBSERVABILITY.TRACES}>
            <ArrowLeft className="h-4 w-4" />
          </Link>
        </Button>
        <div className="flex items-center gap-3">
          <h1 className="text-base font-semibold">Trace</h1>
          <code className="text-xs font-mono bg-muted px-2 py-0.5 rounded text-muted-foreground">
            {traceId?.slice(0, 16)}...
          </code>
        </div>
      </div>
      <div className="flex items-center gap-4 text-xs text-muted-foreground">
        <span className="flex items-center gap-1.5">
          <Clock className="h-3.5 w-3.5" />
          {formatDuration(totalDurationMs)}
        </span>
        <span className="flex items-center gap-1.5">
          <Layers className="h-3.5 w-3.5" />
          {spansCount} spans
        </span>
        <span>{formatTimeAgo(startTime)}</span>
      </div>
    </div>
  );
}
