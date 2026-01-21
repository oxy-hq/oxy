import { Button } from "@/components/ui/shadcn/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/shadcn/select";
import { Network } from "lucide-react";
import { TIME_RANGE_OPTIONS, LIMIT_OPTIONS, type TimeRange } from "../../types";
import SourceFilter from "./SourceFilter";

interface ClustersHeaderProps {
  timeRange: TimeRange;
  limit: number;
  source: string | undefined;
  onTimeRangeChange: (range: TimeRange) => void;
  onLimitChange: (limit: number) => void;
  onSourceChange: (source: string | undefined) => void;
}

export default function ClustersHeader({
  timeRange,
  limit,
  source,
  onTimeRangeChange,
  onLimitChange,
  onSourceChange,
}: ClustersHeaderProps) {
  return (
    <div className="flex justify-between items-center p-4 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="flex items-center gap-3">
        <div className="p-2 rounded-lg bg-primary/10">
          <Network className="h-5 w-5 text-primary" />
        </div>
        <div>
          <h1 className="text-xl font-semibold">Semantic Cluster Map</h1>
          <p className="text-sm text-muted-foreground">
            Analyze user queries by semantic similarity
          </p>
        </div>
      </div>
      <div className="flex items-center gap-3">
        {/* Agent Filter */}
        <SourceFilter onSelect={onSourceChange} selectedSource={source} />

        {/* Limit Selector */}
        <Select
          value={limit.toString()}
          onValueChange={(v) => onLimitChange(parseInt(v))}
        >
          <SelectTrigger className="w-28">
            <SelectValue placeholder="Points" />
          </SelectTrigger>
          <SelectContent>
            {LIMIT_OPTIONS.map((option) => (
              <SelectItem key={option.value} value={option.value.toString()}>
                {option.label} points
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {/* Time Filter */}
        <div className="flex gap-1 border rounded-lg p-1 bg-muted/30">
          {TIME_RANGE_OPTIONS.map((option) => (
            <Button
              key={option.value}
              variant={timeRange === option.value ? "default" : "ghost"}
              size="sm"
              className="h-7 px-3"
              onClick={() => onTimeRangeChange(option.value)}
            >
              {option.label}
            </Button>
          ))}
        </div>
      </div>
    </div>
  );
}
