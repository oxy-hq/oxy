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
import { useMediaQuery, useWindowSize } from "usehooks-ts";
import { cn } from "@/libs/shadcn/utils";
import { buttonVariants } from "@/components/ui/shadcn/utils/button-variants";
import ItemsPerPageFilter from "./ItemsPerPageFilter";

interface ThreadsPaginationProps {
  pagination: PaginationInfo;
  onPageChange: (page: number) => void;
  onLimitChange?: (limit: number) => void;
  currentLimit?: number;
  isLoading?: boolean;
}

export type { ThreadsPaginationProps };

const PaginationButton = ({
  className,
  direction,
  ...props
}: React.ComponentProps<"a"> & {
  direction: "previous" | "next";
}) => {
  const isNext = direction === "next";
  return (
    <a
      aria-label={`Go to ${direction} page`}
      className={cn(
        buttonVariants({ variant: "ghost", size: "icon" }),
        "min-w-0",
        className,
      )}
      {...props}
    >
      {!isNext && <ChevronLeft className="h-4 w-4 shrink-0" />}
      {isNext && <ChevronRight className="h-4 w-4 shrink-0" />}
      <span className="sr-only">{direction}</span>
    </a>
  );
};

const ThreadsPagination: React.FC<ThreadsPaginationProps> = ({
  pagination,
  onPageChange,
  onLimitChange,
  currentLimit,
  isLoading = false,
}) => {
  const { page, total_pages, has_previous, has_next } = pagination;
  const isMobile = useMediaQuery("(max-width: 767px)");
  const { width } = useWindowSize();

  const maxVisiblePages = useMemo(() => {
    if (isMobile) return 3;
    if (width >= 1200) return 10;
    if (width >= 768) return 7;
    return 5;
  }, [isMobile, width]);

  const visiblePages = useMemo(() => {
    // If total pages is small enough, show all pages
    if (total_pages <= maxVisiblePages) {
      return Array.from({ length: total_pages }, (_, i) => i + 1);
    }

    const pages: (number | "ellipsis")[] = [];
    const siblings = Math.floor((maxVisiblePages - 3) / 2); // Reserve space for first, last, and ellipsis

    // Always show first page
    pages.push(1);

    // Calculate start and end of middle section
    let start = Math.max(2, page - siblings);
    let end = Math.min(total_pages - 1, page + siblings);

    // Adjust range if we're near the beginning or end
    if (start <= 3) {
      end = Math.min(total_pages - 1, maxVisiblePages - 1);
      start = 2;
    } else if (end >= total_pages - 2) {
      start = Math.max(2, total_pages - maxVisiblePages + 2);
      end = total_pages - 1;
    }

    // Add ellipsis before middle section if needed
    if (start > 2) {
      pages.push("ellipsis");
    }

    // Add middle pages
    for (let i = start; i <= end; i++) {
      pages.push(i);
    }

    // Add ellipsis after middle section if needed
    if (end < total_pages - 1) {
      pages.push("ellipsis");
    }

    // Always show last page (if more than 1 page)
    if (total_pages > 1) {
      pages.push(total_pages);
    }

    return pages;
  }, [page, total_pages, maxVisiblePages]);

  if (total_pages <= 1 && !onLimitChange) {
    return null;
  }

  return (
    <div
      className={cn(
        "flex",
        isMobile ? "flex-col gap-4" : "items-center justify-between",
      )}
    >
      {onLimitChange && currentLimit && (
        <div className={cn("flex-shrink-0", isMobile && "justify-center")}>
          <ItemsPerPageFilter
            currentLimit={currentLimit}
            onLimitChange={onLimitChange}
            isLoading={isLoading}
          />
        </div>
      )}
      {total_pages > 1 && (
        <div className={cn("flex-shrink-0", isMobile && "justify-center")}>
          <Pagination>
            <PaginationContent className="flex-wrap justify-center">
              <PaginationItem>
                <PaginationButton
                  href="#"
                  direction="previous"
                  onClick={(e: React.MouseEvent) => {
                    e.preventDefault();
                    if (has_previous && !isLoading) {
                      onPageChange(page - 1);
                    }
                  }}
                  className={
                    !has_previous || isLoading
                      ? "pointer-events-none opacity-50"
                      : ""
                  }
                />
              </PaginationItem>
              {visiblePages.map((pageNum, index) => (
                <PaginationItem
                  key={pageNum === "ellipsis" ? `ellipsis-${index}` : pageNum}
                >
                  {pageNum === "ellipsis" ? (
                    <PaginationEllipsis />
                  ) : (
                    <PaginationLink
                      href="#"
                      onClick={(e: React.MouseEvent) => {
                        e.preventDefault();
                        if (!isLoading && typeof pageNum === "number") {
                          onPageChange(pageNum);
                        }
                      }}
                      isActive={pageNum === page}
                      className={cn(
                        isLoading ? "pointer-events-none opacity-50" : "",
                        pageNum === page
                          ? "bg-primary text-primary-foreground shadow-md border-primary hover:bg-primary/90 focus:bg-primary/90"
                          : "",
                      )}
                    >
                      {pageNum}
                    </PaginationLink>
                  )}
                </PaginationItem>
              ))}
              <PaginationItem>
                <PaginationButton
                  href="#"
                  direction="next"
                  onClick={(e: React.MouseEvent) => {
                    e.preventDefault();
                    if (has_next && !isLoading) {
                      onPageChange(page + 1);
                    }
                  }}
                  className={
                    !has_next || isLoading
                      ? "pointer-events-none opacity-50"
                      : ""
                  }
                />
              </PaginationItem>
            </PaginationContent>
          </Pagination>
        </div>
      )}
    </div>
  );
};

export default ThreadsPagination;
