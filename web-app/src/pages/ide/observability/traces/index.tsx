import { Activity } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import TablePagination from "@/components/ui/TablePagination";
import useTraces from "@/hooks/api/traces/useTraces";
import ROUTES from "@/libs/utils/routes";
import PageHeader from "@/pages/ide/components/PageHeader";
import useCurrentProject from "@/stores/useCurrentProject";
import TraceCharts from "./components/Charts";
import TracesList from "./components/TracesList";

const DURATION_OPTIONS = [
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "90d", label: "90d" }
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
  const { data: response, isLoading } = useTraces(PAGE_SIZE, offset, "all", true, durationFilter);

  const { data: chartResponse, isLoading: isChartLoading } = useTraces(
    CHART_LIMIT,
    0,
    "all",
    true,
    durationFilter
  );

  const paginatedTraces = response?.items;
  const total = response?.total ?? 0;
  const totalPages = Math.ceil(total / PAGE_SIZE);

  const handleTraceClick = (traceId: string) => {
    navigate(ROUTES.PROJECT(project?.id || "").IDE.OBSERVABILITY.TRACE(traceId));
  };

  const handleDurationChange = (value: DurationValue) => {
    setDurationFilter(value);
    setCurrentPage(1);
  };

  const durationActions = (
    <Tabs value={durationFilter} onValueChange={(v) => handleDurationChange(v as DurationValue)}>
      <TabsList>
        {DURATION_OPTIONS.map((option) => (
          <TabsTrigger key={option.value} value={option.value}>
            {option.label}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  );

  return (
    <div className='flex h-full flex-col'>
      <div className='flex min-h-0 flex-1 flex-col'>
        <PageHeader icon={Activity} title='Traces' actions={durationActions} />

        <div className='scrollbar-gutter-auto min-h-0 flex-1 overflow-auto p-4'>
          <TraceCharts traces={chartResponse?.items} isLoading={isChartLoading} />

          <TracesList
            isLoading={isLoading}
            traces={paginatedTraces}
            searchQuery=''
            onTraceClick={handleTraceClick}
          />
        </div>

        {/* Pagination */}
        {!isLoading && (
          <div className='p-5'>
            <TablePagination
              currentPage={currentPage}
              totalPages={totalPages}
              totalItems={total}
              pageSize={PAGE_SIZE}
              onPageChange={setCurrentPage}
              itemLabel='traces'
            />
          </div>
        )}
      </div>
    </div>
  );
}
