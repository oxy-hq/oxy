import { Network } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/shadcn/select";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import PageHeader from "@/pages/ide/components/PageHeader";
import { LIMIT_OPTIONS, TIME_RANGE_OPTIONS, type TimeRange } from "../../types";
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
  onSourceChange
}: ClustersHeaderProps) {
  const actions = (
    <>
      {/* Agent Filter */}
      <SourceFilter onSelect={onSourceChange} selectedSource={source} />

      {/* Limit Selector */}
      <Select value={limit.toString()} onValueChange={(v) => onLimitChange(parseInt(v, 10))}>
        <SelectTrigger size='sm'>
          <SelectValue placeholder='Points' />
        </SelectTrigger>
        <SelectContent>
          {LIMIT_OPTIONS.map((option) => (
            <SelectItem
              className='cursor-pointer'
              key={option.value}
              value={option.value.toString()}
            >
              {option.label} points
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      {/* Time Filter */}
      <Tabs value={timeRange} onValueChange={(v) => onTimeRangeChange(v as TimeRange)}>
        <TabsList>
          {TIME_RANGE_OPTIONS.map((option) => (
            <TabsTrigger key={option.value} value={option.value}>
              {option.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </Tabs>
    </>
  );

  return <PageHeader icon={Network} title='Semantic Cluster Map' actions={actions} />;
}
