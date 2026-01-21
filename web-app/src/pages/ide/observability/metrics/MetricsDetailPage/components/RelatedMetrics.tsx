import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/shadcn/card";
import { Sparkles, Hash } from "lucide-react";
import { cn } from "@/libs/shadcn/utils";
import type { MetricDetailResponse } from "@/services/api/metrics";

interface RelatedMetricsProps {
  detailData: MetricDetailResponse;
  onMetricClick: (name: string) => void;
}

function RelatedMetricChip({
  name,
  onClick,
}: {
  name: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "px-3 py-1.5 rounded-lg text-sm font-medium transition-all",
        "bg-muted/50 hover:bg-muted border border-transparent hover:border-primary/30",
        "flex items-center gap-1.5 group",
      )}
    >
      <Hash className="h-3 w-3 text-muted-foreground group-hover:text-primary transition-colors" />
      {name}
    </button>
  );
}

export default function RelatedMetrics({
  detailData,
  onMetricClick,
}: RelatedMetricsProps) {
  const relatedMetrics = detailData.related_metrics;
  return (
    <Card>
      <CardHeader>
        <div className="flex items-center gap-2">
          <Sparkles className="h-5 w-5 text-primary" />
          <CardTitle>Related Metrics</CardTitle>
        </div>
        <CardDescription>Often queried together</CardDescription>
      </CardHeader>
      <CardContent>
        {relatedMetrics.length > 0 ? (
          <div className="flex flex-wrap gap-2">
            {relatedMetrics.map((related) => (
              <RelatedMetricChip
                key={related.name}
                name={related.name}
                onClick={() => onMetricClick(related.name)}
              />
            ))}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            No related metrics found
          </p>
        )}
      </CardContent>
    </Card>
  );
}
