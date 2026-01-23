import { useState } from "react";
import { useNavigate } from "react-router-dom";
import useTraces from "@/hooks/api/traces/useTraces";
import TracesList from "./components/TracesList";
import TraceCharts from "./components/Charts";
import ROUTES from "@/libs/utils/routes";
import useCurrentProject from "@/stores/useCurrentProject";
import TablePagination from "@/components/ui/TablePagination";
import { Button } from "@/components/ui/shadcn/button";
import { Activity } from "lucide-react";
import PageHeader from "@/pages/ide/components/PageHeader";

const DURATION_OPTIONS = [
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "90d", label: "90d" },
] as const;

type DurationValue = (typeof DURATION_OPTIONS)[number]["value"];

const PAGE_SIZE = 10;
const CHART_LIMIT = 500;

export default function TracesPage() {
  const navigate = useNavigate();
  const { project } = useCurrentProject();
  const [durationFilter, setDurationFilter] = useState<DurationValue>("30d");
  const [currentPage, setCurrentPage] = useState(1);

  const offset = (currentPage - 1) * PAGE_SIZE;
  const { data: response, isLoading } = useTraces(
    PAGE_SIZE,
    offset,
    "all",
    true,
    durationFilter,
  );

  const { data: chartResponse, isLoading: isChartLoading } = useTraces(
    CHART_LIMIT,
    0,
    "all",
    true,
    durationFilter,
  );

  const paginatedTraces = response?.items;
  const total = response?.total ?? 0;
  const totalPages = Math.ceil(total / PAGE_SIZE);

  const handleTraceClick = (traceId: string) => {
    navigate(
      ROUTES.PROJECT(project?.id || "").IDE.OBSERVABILITY.TRACE(traceId),
    );
  };

  const handleDurationChange = (value: DurationValue) => {
    setDurationFilter(value);
    setCurrentPage(1);
  };

  const durationActions = (
    <div className="flex gap-1 border rounded-lg p-1 bg-muted/30">
      {DURATION_OPTIONS.map((option) => (
        <Button
          key={option.value}
          variant={durationFilter === option.value ? "default" : "ghost"}
          size="sm"
          className="h-7 px-3"
          onClick={() => handleDurationChange(option.value)}
        >
          {option.label}
        </Button>
      ))}
    </div>
  );

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex flex-col min-h-0">
        <PageHeader
          icon={Activity}
          title="Traces"
          description="View and analyze execution traces"
          actions={durationActions}
        />

        {/* Charts Section */}

        <div className="p-4 flex-1 overflow-auto min-h-0 customScrollbar scrollbar-gutter-auto">
          <TraceCharts
            traces={chartResponse?.items}
            isLoading={isChartLoading}
          />

          <TracesList
            isLoading={isLoading}
            traces={paginatedTraces}
            searchQuery=""
            onTraceClick={handleTraceClick}
          />
        </div>

        {/* Pagination */}
        {!isLoading && (
          <div className="p-5">
            <TablePagination
              currentPage={currentPage}
              totalPages={totalPages}
              totalItems={total}
              pageSize={PAGE_SIZE}
              onPageChange={setCurrentPage}
              itemLabel="traces"
            />
          </div>
        )}
      </div>
    </div>
  );
}
