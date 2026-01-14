import { Card } from "@/components/ui/shadcn/card";
import { Loader2, AlertCircle } from "lucide-react";
import type { Trace } from "@/services/api/traces";
import { TraceCard } from "./TraceCard";

interface TracesListProps {
  isLoading: boolean;
  traces: Trace[] | undefined;
  searchQuery: string;
  onTraceClick: (traceId: string) => void;
}

export function TracesList({
  isLoading,
  traces,
  searchQuery,
  onTraceClick,
}: TracesListProps) {
  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (!traces || traces.length === 0) {
    return (
      <Card className="p-12 text-center">
        <div className="flex flex-col items-center gap-2">
          <AlertCircle className="h-12 w-12 text-muted-foreground" />
          <h3 className="text-lg font-semibold">No traces found</h3>
          <p className="text-sm text-muted-foreground">
            {searchQuery
              ? "Try adjusting your search or filters"
              : "Start running agents to see traces here"}
          </p>
        </div>
      </Card>
    );
  }

  return (
    <div className="space-y-2">
      {traces.map((trace) => (
        <TraceCard
          key={trace.traceId}
          trace={trace}
          onClick={() => onTraceClick(trace.traceId)}
        />
      ))}
    </div>
  );
}

export default TracesList;
