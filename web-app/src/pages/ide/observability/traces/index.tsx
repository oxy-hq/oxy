import { Activity, TriangleAlert } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/shadcn/tabs";
import TablePagination from "@/components/ui/TablePagination";
import { useAuth } from "@/contexts/AuthContext";
import useTraces from "@/hooks/api/traces/useTraces";
import ROUTES from "@/libs/utils/routes";
import PageHeader from "@/pages/ide/components/PageHeader";
import useCurrentOrg from "@/stores/useCurrentOrg";
import useCurrentWorkspace from "@/stores/useCurrentWorkspace";
import TraceCharts from "./components/Charts";
import TracesList from "./components/TracesList";

function ObservabilityNotConfiguredBanner() {
  return (
    <div className='mb-4 flex gap-3 rounded-md border border-warning/30 bg-warning/5 p-4'>
      <div className='flex size-8 shrink-0 items-center justify-center rounded-full bg-warning/15 text-warning'>
        <TriangleAlert className='size-4' />
      </div>
      <div className='flex min-w-0 flex-1 flex-col gap-2'>
        <div>
          <div className='font-medium text-sm'>Observability is not configured</div>
          <p className='mt-0.5 text-muted-foreground text-sm'>
            No traces will be recorded. Pick a backend and restart the server.
          </p>
        </div>
        <pre className='overflow-x-auto rounded-sm bg-muted px-3 py-2 font-mono text-xs leading-relaxed'>
          <span className='text-muted-foreground'># set one of: duckdb, postgres, clickhouse</span>
          {"\n"}
          <span>export </span>
          <span className='text-warning'>OXY_OBSERVABILITY_BACKEND</span>
          <span>=duckdb</span>
        </pre>
      </div>
    </div>
  );
}

const DURATION_OPTIONS = [
  { value: "1h", label: "1h" },
  { value: "24h", label: "24h" },
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "90d", label: "90d" }
] as const;

type DurationValue = (typeof DURATION_OPTIONS)[number]["value"];

const PAGE_SIZE = 10;
const CHART_LIMIT = 500;

export default function TracesPage() {
  const navigate = useNavigate();
  const { workspace: project } = useCurrentWorkspace();
  const orgSlug = useCurrentOrg((s) => s.org?.slug) ?? "";
  const { authConfig } = useAuth();
  const observabilityConfigured = authConfig.observability_enabled;
  const [durationFilter, setDurationFilter] = useState<DurationValue>("30d");
  const [currentPage, setCurrentPage] = useState(1);

  const offset = (currentPage - 1) * PAGE_SIZE;
  const { data: response, isLoading } = useTraces(
    PAGE_SIZE,
    offset,
    "all",
    observabilityConfigured,
    durationFilter
  );

  const { data: chartResponse, isLoading: isChartLoading } = useTraces(
    CHART_LIMIT,
    0,
    "all",
    observabilityConfigured,
    durationFilter
  );

  const paginatedTraces = response?.items;
  const total = response?.total ?? 0;
  const totalPages = Math.ceil(total / PAGE_SIZE);

  const handleTraceClick = (traceId: string) => {
    navigate(
      ROUTES.ORG(orgSlug)
        .WORKSPACE(project?.id || "")
        .IDE.OBSERVABILITY.TRACE(traceId)
    );
  };

  const handleDurationChange = (value: DurationValue) => {
    setDurationFilter(value);
    setCurrentPage(1);
  };

  const durationActions = observabilityConfigured ? (
    <Tabs value={durationFilter} onValueChange={(v) => handleDurationChange(v as DurationValue)}>
      <TabsList>
        {DURATION_OPTIONS.map((option) => (
          <TabsTrigger key={option.value} value={option.value}>
            {option.label}
          </TabsTrigger>
        ))}
      </TabsList>
    </Tabs>
  ) : null;

  return (
    <div className='flex h-full flex-col'>
      <div className='flex min-h-0 flex-1 flex-col'>
        <PageHeader icon={Activity} title='Traces' actions={durationActions} />

        <div className='scrollbar-gutter-auto min-h-0 flex-1 overflow-auto p-4'>
          {observabilityConfigured ? (
            <>
              <TraceCharts traces={chartResponse?.items} isLoading={isChartLoading} />
              <TracesList
                isLoading={isLoading}
                traces={paginatedTraces}
                searchQuery=''
                onTraceClick={handleTraceClick}
              />
            </>
          ) : (
            <ObservabilityNotConfiguredBanner />
          )}
        </div>

        {observabilityConfigured && !isLoading && (
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
