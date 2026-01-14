import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { Button } from "@/components/ui/shadcn/button";
import { ChevronLeft, ChevronRight } from "lucide-react";
import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationLink,
  PaginationEllipsis,
} from "@/components/ui/shadcn/pagination";
import { buttonVariants } from "@/components/ui/shadcn/utils/button-variants";
import useTraces from "@/hooks/api/traces/useTraces";
import TracesList from "./components/TracesList";
import TraceCharts from "./components/Charts";
import { cn } from "@/libs/shadcn/utils";
import ROUTES from "@/libs/utils/routes";
import useCurrentProject from "@/stores/useCurrentProject";

const DURATION_OPTIONS = [
  { value: "1h", label: "1h" },
  { value: "24h", label: "24h" },
  { value: "7d", label: "7d" },
  { value: "30d", label: "30d" },
  { value: "all", label: "All" },
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
  const hasNextPage = currentPage < totalPages;
  const hasPrevPage = currentPage > 1;

  const handleTraceClick = (traceId: string) => {
    navigate(
      ROUTES.PROJECT(project?.id || "").IDE.OBSERVABILITY.TRACE(traceId),
    );
  };

  const handleDurationChange = (value: DurationValue) => {
    setDurationFilter(value);
    setCurrentPage(1);
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex flex-col min-h-0">
        <div className="flex justify-between items-center p-4">
          <DurationFilter
            value={durationFilter}
            onChange={handleDurationChange}
          />
        </div>

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
        {!isLoading && total > 0 && (
          <div className="flex items-center justify-between mt-4 pt-4 border-t p-5">
            <Pagination>
              <PaginationContent className="flex-wrap justify-center">
                <PaginationItem>
                  <Button
                    variant="ghost"
                    disabled={!hasPrevPage}
                    onClick={(e) => {
                      e.preventDefault();
                      if (hasPrevPage) setCurrentPage((prev) => prev - 1);
                    }}
                  >
                    <ChevronLeft />
                  </Button>
                </PaginationItem>
                {generatePaginationItems(currentPage, totalPages).map(
                  (pageNum, idx) => (
                    <PaginationItem
                      key={pageNum === "ellipsis" ? `ellipsis-${idx}` : pageNum}
                    >
                      {pageNum === "ellipsis" ? (
                        <PaginationEllipsis />
                      ) : (
                        <PaginationLink
                          href="#"
                          onClick={(e) => {
                            e.preventDefault();
                            setCurrentPage(pageNum);
                          }}
                          isActive={pageNum === currentPage}
                          className={cn(
                            buttonVariants({
                              variant:
                                pageNum === currentPage ? "default" : "outline",
                            }),
                          )}
                        >
                          {pageNum}
                        </PaginationLink>
                      )}
                    </PaginationItem>
                  ),
                )}
                <PaginationItem>
                  <Button
                    variant="ghost"
                    disabled={!hasNextPage}
                    onClick={(e) => {
                      e.preventDefault();
                      if (hasNextPage) setCurrentPage((prev) => prev + 1);
                    }}
                  >
                    <ChevronRight />
                  </Button>
                </PaginationItem>
              </PaginationContent>
            </Pagination>
          </div>
        )}
      </div>
    </div>
  );
}

// Helper function to generate pagination items with ellipsis
function generatePaginationItems(
  currentPage: number,
  totalPages: number,
): (number | "ellipsis")[] {
  const items: (number | "ellipsis")[] = [];

  if (totalPages <= 7) {
    // Show all pages if total is 7 or less
    for (let i = 1; i <= totalPages; i++) {
      items.push(i);
    }
  } else {
    // Always show first page
    items.push(1);

    if (currentPage > 3) {
      items.push("ellipsis");
    }

    // Show pages around current page
    const start = Math.max(2, currentPage - 1);
    const end = Math.min(totalPages - 1, currentPage + 1);

    for (let i = start; i <= end; i++) {
      items.push(i);
    }

    if (currentPage < totalPages - 2) {
      items.push("ellipsis");
    }

    // Always show last page
    items.push(totalPages);
  }

  return items;
}

interface DurationFilterProps {
  value: DurationValue;
  onChange: (value: DurationValue) => void;
}

function DurationFilter({ value, onChange }: DurationFilterProps) {
  return (
    <div className="flex items-center border rounded-md overflow-hidden">
      {DURATION_OPTIONS.map((option) => (
        <button
          key={option.value}
          onClick={() => onChange(option.value)}
          className={cn(
            "px-3 py-1.5 text-sm transition-colors",
            value === option.value
              ? "bg-primary text-primary-foreground"
              : "bg-background hover:bg-muted text-muted-foreground",
          )}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
