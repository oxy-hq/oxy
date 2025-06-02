import React, { useMemo } from "react";
import {
  Pagination,
  PaginationContent,
  PaginationItem,
  PaginationLink,
  PaginationEllipsis,
} from "@/components/ui/shadcn/pagination";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { PaginationInfo } from "@/types/chat";
import { useMediaQuery } from "usehooks-ts";
import { cn } from "@/libs/shadcn/utils";
import ItemsPerPageFilter from "./ItemsPerPageFilter";
import { Button } from "@/components/ui/shadcn/button";
import { buttonVariants } from "@/components/ui/shadcn/utils/button-variants";

interface ThreadsPaginationProps {
  pagination: PaginationInfo;
  onPageChange: (page: number) => void;
  onLimitChange: (limit: number) => void;
  currentLimit: number;
  isLoading: boolean;
}

function getVisiblePages(
  page: number,
  totalPages: number,
  maxVisible: number,
): (number | "ellipsis")[] {
  if (totalPages <= maxVisible) {
    return Array.from({ length: totalPages }, (_, i) => i + 1);
  }

  const pages: (number | "ellipsis")[] = [1];
  const siblings = Math.floor((maxVisible - 3) / 2);
  let start = Math.max(2, page - siblings);
  let end = Math.min(totalPages - 1, page + siblings);

  if (start <= 3) {
    end = Math.min(totalPages - 1, maxVisible - 1);
    start = 2;
  } else if (end >= totalPages - 2) {
    start = Math.max(2, totalPages - maxVisible + 2);
    end = totalPages - 1;
  }

  if (start > 2) pages.push("ellipsis");
  for (let i = start; i <= end; i++) pages.push(i);
  if (end < totalPages - 1) pages.push("ellipsis");
  if (totalPages > 1) pages.push(totalPages);

  return pages;
}

const ThreadsPagination: React.FC<ThreadsPaginationProps> = ({
  pagination,
  onPageChange,
  onLimitChange,
  currentLimit,
  isLoading = false,
}) => {
  const { page, total_pages, has_previous, has_next } = pagination;
  const isMobile = useMediaQuery("(max-width: 767px)");

  const maxVisiblePages = useMemo(() => {
    if (isMobile) return 3;
    return 5;
  }, [isMobile]);

  const visiblePages = useMemo(
    () => getVisiblePages(page, total_pages, maxVisiblePages),
    [page, total_pages, maxVisiblePages],
  );

  if (total_pages <= 1 && !onLimitChange) return null;

  const handlePageChange = (pageNum: number) => {
    if (pageNum !== page && !isLoading) onPageChange(pageNum);
  };

  return (
    <div
      className={cn(
        "flex w-full max-w-page-content mx-auto px-2 items-center justify-between",
      )}
    >
      <ItemsPerPageFilter
        currentLimit={currentLimit}
        onLimitChange={onLimitChange}
        isLoading={isLoading}
      />

      {total_pages > 1 && (
        <div className={cn("flex-shrink-0", isMobile && "justify-center")}>
          <Pagination>
            <PaginationContent className="flex-wrap justify-center">
              <PaginationItem>
                <Button
                  variant="ghost"
                  disabled={!has_previous || isLoading}
                  onClick={(e) => {
                    e.preventDefault();
                    if (has_previous && !isLoading) onPageChange(page - 1);
                  }}
                >
                  <ChevronLeft />
                </Button>
              </PaginationItem>
              {visiblePages.map((pageNum, idx) => (
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
                        handlePageChange(pageNum as number);
                      }}
                      isActive={pageNum === page}
                      className={cn(
                        isLoading ? "pointer-events-none opacity-50" : "",
                        buttonVariants({
                          variant: pageNum === page ? "default" : "outline",
                        }),
                      )}
                    >
                      {pageNum}
                    </PaginationLink>
                  )}
                </PaginationItem>
              ))}
              <PaginationItem>
                <Button
                  variant="ghost"
                  disabled={!has_next || isLoading}
                  onClick={(e) => {
                    e.preventDefault();
                    if (has_next && !isLoading) onPageChange(page + 1);
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
  );
};

export default ThreadsPagination;
